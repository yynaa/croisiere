//! End-to-end test: a small banking application built on every layer of the
//! framework, from domain types to units of work.
//!
//! - Domain: an `Account` entity with a `Balance` value object, tied together
//!   by `AccountAggregate`, and a `TransferService` working across two
//!   accounts.
//! - Persistence: `AccountSnapshot`, filters and sorters, an in-memory
//!   database, a repository that reads it and stages writes, and a unit of
//!   work that commits staged writes as one transaction with optimistic
//!   concurrency checks.
//! - Application: commands (open, deposit, withdraw, transfer, close) and a
//!   query (list accounts), each run on a unit of work from a factory.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    rc::Rc,
    sync::{Arc, Mutex},
};

use croisiere::{
    application::{
        AbstractActionableSnapshot, Command, Filter, Pagination, Query, ReadRepository,
        RepositoryDropped, Snapshot, SnapshotAction, Sorter, UnitOfWork, UnitOfWorkFactory,
        VersionedSnapshot, WriteRepository,
    },
    domain::{
        DomainService, Entity, ValueObject, aggregate_functions, domain_service_functions,
        make_aggregate,
    },
};

// =============================================================================
// Domain
// =============================================================================

#[derive(Debug, Default, Entity)]
pub struct Account {
    pub id: u32,
    pub owner: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct Balance {
    pub cents: i64,
}

make_aggregate!(
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account { balance: Balance }
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountEvent {
    Opened { id: u32, owner: String },
    Deposited { id: u32, cents: i64 },
    Withdrawn { id: u32, cents: i64 },
    Closed { id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    EmptyOwner,
    InvalidAmount,
    InsufficientFunds,
    NonZeroBalance,
    SameAccount,
}

#[aggregate_functions]
impl AccountAggregate {
    pub fn open(id: u32, owner: String, cents: i64) -> CreationResult {
        if owner.is_empty() {
            return Err(AccountError::EmptyOwner);
        }
        if cents < 0 {
            return Err(AccountError::InvalidAmount);
        }
        let events = vec![AccountEvent::Opened {
            id,
            owner: owner.clone(),
        }];
        let aggregate = Self::builder()
            .root(Account { id, owner })
            .balance(Balance { cents })
            .build();
        Ok((aggregate, events))
    }

    pub fn deposit(&mut self, cents: i64) -> ActionResult {
        Self::check_amount(cents)?;
        self.balance.root.cents += cents;
        Ok(vec![AccountEvent::Deposited {
            id: self.root.id,
            cents,
        }])
    }

    pub fn withdraw(&mut self, cents: i64) -> ActionResult {
        Self::check_amount(cents)?;
        if self.balance.root.cents < cents {
            return Err(AccountError::InsufficientFunds);
        }
        self.balance.root.cents -= cents;
        Ok(vec![AccountEvent::Withdrawn {
            id: self.root.id,
            cents,
        }])
    }

    pub fn close(&mut self) -> ActionResult {
        self.check_can_close()?;
        Ok(vec![AccountEvent::Closed { id: self.root.id }])
    }

    fn check_can_close(&self) -> CheckResult {
        if self.balance.root.cents != 0 {
            return Err(AccountError::NonZeroBalance);
        }
        Ok(())
    }

    fn check_amount(cents: i64) -> CheckResult {
        if cents <= 0 {
            return Err(AccountError::InvalidAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Default, DomainService)]
#[service(event = AccountEvent, error = AccountError)]
pub struct TransferService;

#[domain_service_functions]
impl TransferService {
    /// Moves `cents` from one account to the other. On error, neither account
    /// has changed.
    pub fn transfer(
        &self,
        from: &mut AccountAggregate,
        to: &mut AccountAggregate,
        cents: i64,
    ) -> Result<Vec<AccountEvent>, AccountError> {
        if from.root.id == to.root.id {
            return Err(AccountError::SameAccount);
        }
        // `withdraw` checks the amount, so `deposit` can't fail after it.
        let mut events = from.withdraw(cents)?;
        events.extend(to.deposit(cents)?);
        Ok(events)
    }
}

// =============================================================================
// Snapshot
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Snapshot)]
#[snapshot(aggregate = AccountAggregate, filter = AccountFilter, sorter = AccountSorter)]
pub struct AccountSnapshot {
    pub id: u32,
    pub owner: String,
    pub cents: i64,
}

/// A stored snapshot that breaks the aggregate's invariants.
#[derive(Debug)]
pub struct InvalidSnapshot;

impl From<AccountAggregate> for AccountSnapshot {
    fn from(aggregate: AccountAggregate) -> Self {
        Self {
            id: aggregate.root.id,
            owner: aggregate.root.owner,
            cents: aggregate.balance.root.cents,
        }
    }
}

impl TryFrom<AccountSnapshot> for AccountAggregate {
    type Error = InvalidSnapshot;

    fn try_from(snapshot: AccountSnapshot) -> Result<Self, Self::Error> {
        if snapshot.owner.is_empty() || snapshot.cents < 0 {
            return Err(InvalidSnapshot);
        }
        Ok(Self::builder()
            .root(Account {
                id: snapshot.id,
                owner: snapshot.owner,
            })
            .balance(Balance {
                cents: snapshot.cents,
            })
            .build())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Filter)]
pub enum AccountFilter {
    ById(u32),
    ByOwner(String),
}

impl AccountFilter {
    fn matches(&self, snapshot: &AccountSnapshot) -> bool {
        match self {
            Self::ById(id) => snapshot.id == *id,
            Self::ByOwner(owner) => snapshot.owner == *owner,
        }
    }
}

#[derive(Debug, Clone, Copy, Sorter)]
pub enum AccountSorter {
    ById,
    ByBalanceDescending,
}

// =============================================================================
// Errors
// =============================================================================

/// The single error type of the application, shared by repositories and units
/// of work so actions on any snapshot can be registered on the same unit of
/// work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Domain(AccountError),
    NotFound(u32),
    AlreadyExists(u32),
    /// The account changed since it was loaded.
    Conflict(u32),
    InvalidSnapshot,
    RepositoryDropped,
}

impl From<AccountError> for AppError {
    fn from(error: AccountError) -> Self {
        Self::Domain(error)
    }
}

impl From<InvalidSnapshot> for AppError {
    fn from(_: InvalidSnapshot) -> Self {
        Self::InvalidSnapshot
    }
}

impl From<RepositoryDropped> for AppError {
    fn from(_: RepositoryDropped) -> Self {
        Self::RepositoryDropped
    }
}

// =============================================================================
// Infrastructure: database, repository, unit of work
// =============================================================================

/// Committed state, shared by every unit of work. Keyed by id, so storage
/// order is id order.
#[derive(Default)]
pub struct Database {
    accounts: BTreeMap<u32, VersionedSnapshot<AccountSnapshot>>,
}

/// A write waiting for its unit of work to commit.
enum StagedWrite {
    Save(VersionedSnapshot<AccountSnapshot>),
    Delete(VersionedSnapshot<AccountSnapshot>),
}

impl Database {
    /// Applies `write` if the stored version is the one it was made from (one
    /// less than its own, as `make_actionable` increments it).
    fn apply(&mut self, write: StagedWrite) -> Result<(), AppError> {
        match write {
            StagedWrite::Save(vs) => {
                let id = vs.snapshot.id;
                // A new account is saved with version 1, from version 0.
                let stored = self.accounts.get(&id).map_or(0, |s| s.version);
                if stored + 1 != vs.version {
                    return Err(AppError::Conflict(id));
                }
                self.accounts.insert(id, vs);
            }
            StagedWrite::Delete(vs) => {
                let id = vs.snapshot.id;
                match self.accounts.get(&id) {
                    Some(stored) if stored.version + 1 == vs.version => {
                        self.accounts.remove(&id);
                    }
                    _ => return Err(AppError::Conflict(id)),
                }
            }
        }
        Ok(())
    }
}

/// Reads committed accounts, and stages writes for its unit of work.
pub struct AccountRepository {
    database: Arc<Mutex<Database>>,
    staged: Rc<RefCell<Vec<StagedWrite>>>,
}

impl ReadRepository<AccountSnapshot> for AccountRepository {
    type Error = AppError;

    fn get(
        &mut self,
        filters: HashSet<AccountFilter>,
        sorter: Option<AccountSorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<AccountSnapshot>>, AppError> {
        let database = self.database.lock().unwrap();
        let mut found: Vec<_> = database
            .accounts
            .values()
            .filter(|vs| filters.iter().all(|f| f.matches(&vs.snapshot)))
            .cloned()
            .collect();
        drop(database);

        match sorter {
            None | Some(AccountSorter::ById) => found.sort_by_key(|vs| vs.snapshot.id),
            Some(AccountSorter::ByBalanceDescending) => {
                found.sort_by_key(|vs| (std::cmp::Reverse(vs.snapshot.cents), vs.snapshot.id))
            }
        }
        Ok(match pagination {
            Some(Pagination { limit, offset }) => {
                found.into_iter().skip(offset).take(limit).collect()
            }
            None => found,
        })
    }

    fn count(
        &mut self,
        filters: HashSet<AccountFilter>,
        sorter: Option<AccountSorter>,
    ) -> Result<usize, AppError> {
        Ok(self.get(filters, sorter, None)?.len())
    }
}

impl WriteRepository<AccountSnapshot> for AccountRepository {
    type Error = AppError;

    fn save(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<AccountSnapshot>>,
    ) -> Result<(), AppError> {
        let mut staged = self.staged.borrow_mut();
        staged.extend(snapshots.into_iter().cloned().map(StagedWrite::Save));
        Ok(())
    }

    fn delete(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<AccountSnapshot>>,
    ) -> Result<(), AppError> {
        let mut staged = self.staged.borrow_mut();
        staged.extend(snapshots.into_iter().cloned().map(StagedWrite::Delete));
        Ok(())
    }
}

/// The repositories of a unit of work.
pub struct Registry {
    pub accounts: Rc<RefCell<AccountRepository>>,
}

type Actions = Vec<Box<dyn AbstractActionableSnapshot<Error = AppError>>>;

pub struct InMemoryUnitOfWork {
    database: Arc<Mutex<Database>>,
    registry: Registry,
    staged: Rc<RefCell<Vec<StagedWrite>>>,
    actions: RefCell<Actions>,
}

impl UnitOfWork for InMemoryUnitOfWork {
    type Error = AppError;
    type Registry = Registry;

    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn register_action(
        &self,
        actionable_snapshot: Box<dyn AbstractActionableSnapshot<Error = AppError>>,
    ) {
        self.actions.borrow_mut().push(actionable_snapshot);
    }

    /// Stages every registered action, then applies the staged writes to a
    /// copy of the database, which replaces it only if all of them succeed.
    fn commit_and_rollback(&self) -> Result<(), AppError> {
        let actions = std::mem::take(&mut *self.actions.borrow_mut());
        let result = actions.into_iter().try_for_each(|mut action| action.act());
        let staged = std::mem::take(&mut *self.staged.borrow_mut());
        result?;

        let mut database = self.database.lock().unwrap();
        let mut transaction = Database {
            accounts: database.accounts.clone(),
        };
        for write in staged {
            transaction.apply(write)?;
        }
        database.accounts = transaction.accounts;
        Ok(())
    }
}

/// Starts units of work on a shared database. Cloning it shares the database.
#[derive(Clone, Default)]
pub struct InMemoryFactory {
    database: Arc<Mutex<Database>>,
}

pub type Uow = Box<dyn UnitOfWork<Error = AppError, Registry = Registry>>;

impl UnitOfWorkFactory for InMemoryFactory {
    type Error = AppError;
    type Registry = Registry;

    fn begin(&self) -> Uow {
        let staged = Rc::new(RefCell::new(Vec::new()));
        let accounts = AccountRepository {
            database: Arc::clone(&self.database),
            staged: Rc::clone(&staged),
        };
        Box::new(InMemoryUnitOfWork {
            database: Arc::clone(&self.database),
            registry: Registry {
                accounts: Rc::new(RefCell::new(accounts)),
            },
            staged,
            actions: RefCell::new(Vec::new()),
        })
    }
}

impl InMemoryFactory {
    /// The committed state of an account, bypassing units of work.
    fn stored(&self, id: u32) -> Option<(AccountSnapshot, u128)> {
        let database = self.database.lock().unwrap();
        database
            .accounts
            .get(&id)
            .map(|vs| (vs.snapshot.clone(), vs.version))
    }

    /// Writes an account directly, bypassing units of work and invariants.
    fn force_store(&self, snapshot: AccountSnapshot, version: u128) {
        let mut database = self.database.lock().unwrap();
        database
            .accounts
            .insert(snapshot.id, VersionedSnapshot { snapshot, version });
    }
}

// =============================================================================
// Application: commands and queries
// =============================================================================

fn load(
    repository: &Rc<RefCell<AccountRepository>>,
    id: u32,
) -> Result<VersionedSnapshot<AccountSnapshot>, AppError> {
    repository
        .borrow_mut()
        .get(HashSet::from([AccountFilter::ById(id)]), None, None)?
        .pop()
        .ok_or(AppError::NotFound(id))
}

pub struct OpenAccount {
    pub id: u32,
    pub owner: String,
    pub cents: i64,
}

impl Command for OpenAccount {
    type Context = Uow;
    type Response = Vec<AccountEvent>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "OpenAccount"
    }

    fn handle(&self, uow: Uow) -> Result<Vec<AccountEvent>, AppError> {
        let repository = &uow.registry().accounts;
        let existing = repository
            .borrow_mut()
            .count(HashSet::from([AccountFilter::ById(self.id)]), None)?;
        if existing > 0 {
            return Err(AppError::AlreadyExists(self.id));
        }

        let (aggregate, events) = AccountAggregate::open(self.id, self.owner.clone(), self.cents)?;
        let actionable = VersionedSnapshot::<AccountSnapshot>::assemble(aggregate, 0)
            .make_actionable(Rc::downgrade(repository), SnapshotAction::Save);
        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;
        Ok(events)
    }
}

/// Runs one aggregate function on one account and saves or deletes it.
fn act_on_account(
    uow: &Uow,
    id: u32,
    action: SnapshotAction,
    f: impl FnOnce(&mut AccountAggregate) -> Result<Vec<AccountEvent>, AccountError>,
) -> Result<Vec<AccountEvent>, AppError> {
    let repository = &uow.registry().accounts;
    let (actionable, events) = load(repository, id)?
        .do_and_make_actionable::<AccountEvent, AppError, _>(
            Rc::downgrade(repository),
            action,
            f,
        )?;
    uow.register_action(Box::new(actionable));
    uow.commit_and_rollback()?;
    Ok(events)
}

pub struct Deposit {
    pub id: u32,
    pub cents: i64,
}

impl Command for Deposit {
    type Context = Uow;
    type Response = Vec<AccountEvent>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "Deposit"
    }

    fn handle(&self, uow: Uow) -> Result<Vec<AccountEvent>, AppError> {
        act_on_account(&uow, self.id, SnapshotAction::Save, |a| {
            a.deposit(self.cents)
        })
    }
}

pub struct Withdraw {
    pub id: u32,
    pub cents: i64,
}

impl Command for Withdraw {
    type Context = Uow;
    type Response = Vec<AccountEvent>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "Withdraw"
    }

    fn handle(&self, uow: Uow) -> Result<Vec<AccountEvent>, AppError> {
        act_on_account(&uow, self.id, SnapshotAction::Save, |a| {
            a.withdraw(self.cents)
        })
    }
}

pub struct CloseAccount {
    pub id: u32,
}

impl Command for CloseAccount {
    type Context = Uow;
    type Response = Vec<AccountEvent>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "CloseAccount"
    }

    fn handle(&self, uow: Uow) -> Result<Vec<AccountEvent>, AppError> {
        act_on_account(
            &uow,
            self.id,
            SnapshotAction::Delete,
            AccountAggregate::close,
        )
    }
}

pub struct Transfer {
    pub from: u32,
    pub to: u32,
    pub cents: i64,
}

impl Transfer {
    /// Everything but the commit: registers both accounts on `uow`.
    fn prepare(&self, uow: &Uow) -> Result<Vec<AccountEvent>, AppError> {
        let repository = &uow.registry().accounts;
        let (mut from, from_version) = load(repository, self.from)?.dismantle()?;
        let (mut to, to_version) = load(repository, self.to)?.dismantle()?;

        let events = TransferService.transfer(&mut from, &mut to, self.cents)?;

        for (aggregate, version) in [(from, from_version), (to, to_version)] {
            let actionable = VersionedSnapshot::<AccountSnapshot>::assemble(aggregate, version)
                .make_actionable(Rc::downgrade(repository), SnapshotAction::Save);
            uow.register_action(Box::new(actionable));
        }
        Ok(events)
    }
}

impl Command for Transfer {
    type Context = Uow;
    type Response = Vec<AccountEvent>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "Transfer"
    }

    fn handle(&self, uow: Uow) -> Result<Vec<AccountEvent>, AppError> {
        let events = self.prepare(&uow)?;
        uow.commit_and_rollback()?;
        Ok(events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountView {
    pub id: u32,
    pub owner: String,
    pub cents: i64,
    pub version: u128,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    /// Number of matching accounts, ignoring pagination.
    pub total: usize,
    pub items: Vec<AccountView>,
}

#[derive(Default)]
pub struct ListAccounts {
    pub filters: Vec<AccountFilter>,
    pub sorter: Option<AccountSorter>,
    /// `(limit, offset)`.
    pub page: Option<(usize, usize)>,
}

impl Query for ListAccounts {
    type Context = Uow;
    type Response = Page;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "ListAccounts"
    }

    fn handle(&self, uow: Uow) -> Result<Page, AppError> {
        let mut repository = uow.registry().accounts.borrow_mut();
        let filters: HashSet<_> = self.filters.iter().cloned().collect();

        let total = repository.count(filters.clone(), self.sorter)?;
        let pagination = self
            .page
            .map(|(limit, offset)| Pagination { limit, offset });
        let items = repository
            .get(filters, self.sorter, pagination)?
            .into_iter()
            .map(|vs| AccountView {
                id: vs.snapshot.id,
                owner: vs.snapshot.owner,
                cents: vs.snapshot.cents,
                version: vs.version,
            })
            .collect();
        Ok(Page { total, items })
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Runs `command` on a new unit of work, as a dispatcher would.
fn execute<C>(
    factory: &impl UnitOfWorkFactory<Error = AppError, Registry = Registry>,
    command: C,
) -> Result<C::Response, AppError>
where
    C: Command<Context = Uow, Error = AppError>,
{
    command.handle(factory.begin())
}

fn query<Q>(
    factory: &impl UnitOfWorkFactory<Error = AppError, Registry = Registry>,
    query: Q,
) -> Result<Q::Response, AppError>
where
    Q: Query<Context = Uow, Error = AppError>,
{
    query.handle(factory.begin())
}

fn open(factory: &InMemoryFactory, id: u32, owner: &str, cents: i64) {
    execute(
        factory,
        OpenAccount {
            id,
            owner: owner.to_owned(),
            cents,
        },
    )
    .unwrap();
}

fn snapshot(id: u32, owner: &str, cents: i64) -> AccountSnapshot {
    AccountSnapshot {
        id,
        owner: owner.to_owned(),
        cents,
    }
}

// --- Opening -----------------------------------------------------------------

#[test]
fn opening_an_account_stores_it_at_version_1() {
    let factory = InMemoryFactory::default();

    let events = execute(
        &factory,
        OpenAccount {
            id: 1,
            owner: "ada".to_owned(),
            cents: 500,
        },
    )
    .unwrap();

    assert_eq!(
        events,
        vec![AccountEvent::Opened {
            id: 1,
            owner: "ada".to_owned()
        }]
    );
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 500), 1)));
}

#[test]
fn opening_an_existing_account_fails() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);

    let result = execute(
        &factory,
        OpenAccount {
            id: 1,
            owner: "bob".to_owned(),
            cents: 0,
        },
    );

    assert_eq!(result, Err(AppError::AlreadyExists(1)));
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 500), 1)));
}

#[test]
fn opening_an_invalid_account_stores_nothing() {
    let factory = InMemoryFactory::default();

    let result = execute(
        &factory,
        OpenAccount {
            id: 1,
            owner: String::new(),
            cents: 0,
        },
    );

    assert_eq!(result, Err(AppError::Domain(AccountError::EmptyOwner)));
    assert_eq!(factory.stored(1), None);
}

// --- Single-account commands -------------------------------------------------

#[test]
fn deposit_and_withdraw_update_balance_and_version() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);

    let deposited = execute(&factory, Deposit { id: 1, cents: 250 }).unwrap();
    let withdrawn = execute(&factory, Withdraw { id: 1, cents: 100 }).unwrap();

    assert_eq!(
        deposited,
        vec![AccountEvent::Deposited { id: 1, cents: 250 }]
    );
    assert_eq!(
        withdrawn,
        vec![AccountEvent::Withdrawn { id: 1, cents: 100 }]
    );
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 650), 3)));
}

#[test]
fn a_refused_action_changes_nothing() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 100);

    let result = execute(&factory, Withdraw { id: 1, cents: 101 });

    assert_eq!(
        result,
        Err(AppError::Domain(AccountError::InsufficientFunds))
    );
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 100), 1)));
}

#[test]
fn acting_on_a_missing_account_fails() {
    let factory = InMemoryFactory::default();

    assert_eq!(
        execute(&factory, Deposit { id: 9, cents: 1 }),
        Err(AppError::NotFound(9))
    );
}

#[test]
fn an_invalid_stored_snapshot_is_reported() {
    let factory = InMemoryFactory::default();
    factory.force_store(snapshot(1, "ada", -5), 1);

    let result = execute(&factory, Deposit { id: 1, cents: 10 });

    assert_eq!(result, Err(AppError::InvalidSnapshot));
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", -5), 1)));
}

#[test]
fn closing_deletes_an_empty_account() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 0);

    let events = execute(&factory, CloseAccount { id: 1 }).unwrap();

    assert_eq!(events, vec![AccountEvent::Closed { id: 1 }]);
    assert_eq!(factory.stored(1), None);
    assert_eq!(
        execute(&factory, Deposit { id: 1, cents: 1 }),
        Err(AppError::NotFound(1))
    );
}

#[test]
fn closing_an_account_with_money_fails() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 1);

    assert_eq!(
        execute(&factory, CloseAccount { id: 1 }),
        Err(AppError::Domain(AccountError::NonZeroBalance))
    );
    assert!(factory.stored(1).is_some());
}

// --- Transfers (domain service, two aggregates in one unit of work) ----------

#[test]
fn transfer_moves_money_between_accounts() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);
    open(&factory, 2, "bob", 0);

    let events = execute(
        &factory,
        Transfer {
            from: 1,
            to: 2,
            cents: 200,
        },
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            AccountEvent::Withdrawn { id: 1, cents: 200 },
            AccountEvent::Deposited { id: 2, cents: 200 },
        ]
    );
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 300), 2)));
    assert_eq!(factory.stored(2), Some((snapshot(2, "bob", 200), 2)));
}

#[test]
fn a_refused_transfer_changes_neither_account() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 100);
    open(&factory, 2, "bob", 0);

    let result = execute(
        &factory,
        Transfer {
            from: 1,
            to: 2,
            cents: 500,
        },
    );

    assert_eq!(
        result,
        Err(AppError::Domain(AccountError::InsufficientFunds))
    );
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 100), 1)));
    assert_eq!(factory.stored(2), Some((snapshot(2, "bob", 0), 1)));
}

#[test]
fn transfer_to_the_same_account_fails() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 100);

    assert_eq!(
        execute(
            &factory,
            Transfer {
                from: 1,
                to: 1,
                cents: 10,
            }
        ),
        Err(AppError::Domain(AccountError::SameAccount))
    );
}

// --- Unit of work: transactions and optimistic concurrency -------------------

#[test]
fn concurrent_change_makes_the_later_commit_fail() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 100);

    // Both units of work load version 1 and deposit on it.
    let deposit_on = |uow: &Uow, cents: i64| {
        let repository = &uow.registry().accounts;
        let (actionable, _) = load(repository, 1)
            .unwrap()
            .do_and_make_actionable::<AccountEvent, AppError, _>(
                Rc::downgrade(repository),
                SnapshotAction::Save,
                |a| a.deposit(cents),
            )
            .unwrap();
        uow.register_action(Box::new(actionable));
    };
    let first = factory.begin();
    let second = factory.begin();
    deposit_on(&first, 10);
    deposit_on(&second, 20);

    // The first commit wins; the second was made from a stale version.
    assert_eq!(first.commit_and_rollback(), Ok(()));
    assert_eq!(second.commit_and_rollback(), Err(AppError::Conflict(1)));
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 110), 2)));
}

#[test]
fn a_failing_write_rolls_back_the_whole_commit() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);
    open(&factory, 2, "bob", 0);

    // The transfer loads both accounts at version 1 and registers them...
    let uow = factory.begin();
    Transfer {
        from: 1,
        to: 2,
        cents: 200,
    }
    .prepare(&uow)
    .unwrap();

    // ...then account 2 changes before the transfer commits.
    execute(&factory, Deposit { id: 2, cents: 5 }).unwrap();

    // Account 1 is written first and would succeed on its own, but account 2
    // conflicts, so neither is written.
    assert_eq!(uow.commit_and_rollback(), Err(AppError::Conflict(2)));
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 500), 1)));
    assert_eq!(factory.stored(2), Some((snapshot(2, "bob", 5), 2)));
}

#[test]
fn nothing_is_written_until_commit() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);

    let uow = factory.begin();
    let repository = &uow.registry().accounts;
    let (actionable, _) = load(repository, 1)
        .unwrap()
        .do_and_make_actionable::<AccountEvent, AppError, _>(
            Rc::downgrade(repository),
            SnapshotAction::Save,
            |a| a.deposit(1),
        )
        .unwrap();
    uow.register_action(Box::new(actionable));

    // Registered but not committed: dropping the unit of work discards it.
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 500), 1)));
    drop(uow);
    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 500), 1)));
}

#[test]
fn committing_twice_does_not_write_twice() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 0);

    let uow = factory.begin();
    act_on_account(&uow, 1, SnapshotAction::Save, |a| a.deposit(10)).unwrap();
    // Actions are consumed by the first commit.
    uow.commit_and_rollback().unwrap();

    assert_eq!(factory.stored(1), Some((snapshot(1, "ada", 10), 2)));
}

#[test]
fn the_factory_is_shared_across_threads() {
    const THREADS: usize = 4;
    const DEPOSITS: usize = 25;

    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 0);

    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..DEPOSITS {
                    // Retry on conflict, as a client of the application would.
                    loop {
                        match execute(&factory, Deposit { id: 1, cents: 1 }) {
                            Ok(_) => break,
                            Err(AppError::Conflict(1)) => continue,
                            Err(error) => panic!("unexpected error: {error:?}"),
                        }
                    }
                }
            });
        }
    });

    let total = (THREADS * DEPOSITS) as i64;
    assert_eq!(
        factory.stored(1),
        Some((snapshot(1, "ada", total), 1 + total as u128))
    );
}

// --- Queries -----------------------------------------------------------------

fn seed(factory: &InMemoryFactory) {
    open(factory, 3, "ada", 300);
    open(factory, 1, "bob", 100);
    open(factory, 4, "ada", 50);
    open(factory, 2, "ada", 300);
}

fn ids(page: &Page) -> Vec<u32> {
    page.items.iter().map(|item| item.id).collect()
}

#[test]
fn listing_without_options_returns_everything_in_storage_order() {
    let factory = InMemoryFactory::default();
    seed(&factory);

    let page = query(&factory, ListAccounts::default()).unwrap();

    assert_eq!(page.total, 4);
    assert_eq!(ids(&page), vec![1, 2, 3, 4]);
    assert_eq!(
        page.items[0],
        AccountView {
            id: 1,
            owner: "bob".to_owned(),
            cents: 100,
            version: 1
        }
    );
}

#[test]
fn listing_filters_and_sorts() {
    let factory = InMemoryFactory::default();
    seed(&factory);

    let page = query(
        &factory,
        ListAccounts {
            filters: vec![AccountFilter::ByOwner("ada".to_owned())],
            sorter: Some(AccountSorter::ByBalanceDescending),
            page: None,
        },
    )
    .unwrap();

    assert_eq!(page.total, 3);
    // Ties on balance are ordered by id.
    assert_eq!(ids(&page), vec![2, 3, 4]);
}

#[test]
fn filters_are_combined() {
    let factory = InMemoryFactory::default();
    seed(&factory);

    let page = query(
        &factory,
        ListAccounts {
            filters: vec![
                AccountFilter::ByOwner("ada".to_owned()),
                AccountFilter::ById(1),
            ],
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());
}

#[test]
fn listing_paginates_but_counts_every_match() {
    let factory = InMemoryFactory::default();
    seed(&factory);

    let page = query(
        &factory,
        ListAccounts {
            sorter: Some(AccountSorter::ById),
            page: Some((2, 1)),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(page.total, 4);
    assert_eq!(ids(&page), vec![2, 3]);
}

#[test]
fn queries_see_committed_changes() {
    let factory = InMemoryFactory::default();
    open(&factory, 1, "ada", 500);
    open(&factory, 2, "bob", 0);
    execute(
        &factory,
        Transfer {
            from: 1,
            to: 2,
            cents: 500,
        },
    )
    .unwrap();
    execute(&factory, CloseAccount { id: 1 }).unwrap();

    let page = query(&factory, ListAccounts::default()).unwrap();

    assert_eq!(
        page.items,
        vec![AccountView {
            id: 2,
            owner: "bob".to_owned(),
            cents: 500,
            version: 2
        }]
    );
}

// --- Names -------------------------------------------------------------------

#[test]
fn names_identify_each_type_and_request() {
    use croisiere::domain::Aggregate;

    assert_eq!(Account::NAME, "Account");
    assert_eq!(Balance::NAME, "Balance");
    assert_eq!(AccountAggregate::NAME, "AccountAggregate");
    assert_eq!(TransferService::NAME, "TransferService");
    assert_eq!(AccountSnapshot::NAME, "AccountSnapshot");
    assert_eq!(Deposit { id: 0, cents: 0 }.name(), "Deposit");
    assert_eq!(ListAccounts::default().name(), "ListAccounts");
}
