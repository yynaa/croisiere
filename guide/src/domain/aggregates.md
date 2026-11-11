# Domain: Aggregates

An aggregate is a group of [entities](./entities.md) and [value objects](./value_objects.md) that
change together. It has a root, an entity which gives the aggregate its identity, and every change to
anything inside the aggregate goes through it. The aggregate is the unit the rest of your application
loads, changes and saves.

In `croisiere`, aggregates are described by the [`Aggregate`] trait. You never implement it yourself:
aggregates are declared with the [`make_aggregate!`] macro, and their behaviour is written in an impl
block marked with [`#[aggregate_functions]`](#aggregate-functions).

## Declaring an aggregate

Give [`make_aggregate!`] the root, the events the aggregate emits, the errors it fails with, and the
parts it is made of:

```rust,ignore
use croisiere::domain::{Entity, ValueObject, make_aggregate};

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
```

This generates one wrapper struct per part, each holding its domain type in a `root` field:

```rust,ignore
pub struct AccountAggregate {
    pub root: Account,
    pub balance: AccountAggregateBalance,
}

pub struct AccountAggregateBalance {
    pub root: Balance,
}

impl croisiere::domain::Aggregate for AccountAggregate {
    const NAME: &'static str = "AccountAggregate";
    type Event = AccountEvent;
    type Error = AccountError;
}
```

The balance is reached through `aggregate.balance.root`, and the account itself through
`aggregate.root`. Only the top-level wrapper is an aggregate; the nested wrappers are views inside it.

Wrappers are named `<root>Aggregate` followed by the path to the part, with field names in
PascalCase: a field `line_items` gives `AccountAggregateLineItems`. Two paths that would give the
same name, such as `foo_bar` and `foo { bar }`, are a compile error.

Several aggregates can be declared in the same `make_aggregate!` call. Commas between fields are
optional.

### Events and errors

Every aggregate must have `#[emits = EventType]` and `#[fails = ErrorType]` before its root. They
become [`Aggregate::Event`] and [`Aggregate::Error`], and are used by the
[aggregate functions](#aggregate-functions):

```rust,ignore
#[derive(Debug, Clone)]
pub enum AccountEvent {
    Opened { id: u32, owner: String },
    Deposited { id: u32, cents: i64 },
}

#[derive(Debug)]
pub enum AccountError {
    EmptyOwner,
    InvalidAmount,
}
```

### Nested parts

A part can have parts of its own, written in braces after its type:

```rust,ignore
make_aggregate!(
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account {
        balance: Balance,
        card: Card { limit: Money },
    }
);
// pub struct AccountAggregateCard { pub root: Card, pub limit: AccountAggregateCardLimit }
// pub struct AccountAggregateCardLimit { pub root: Money }
```

When a group of parts has no domain type of its own, mark it `#[rootless]`: its wrapper is generated
without a `root` field.

```rust,ignore
make_aggregate!(
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account { #[rootless] limits: Limits { daily: Money, monthly: Money } }
);
// pub struct AccountAggregateLimits {
//     pub daily: AccountAggregateLimitsDaily,
//     pub monthly: AccountAggregateLimitsMonthly,
// }
```

### Optional parts and collections

A part written as `Container<X>` holds the wrapper of `X` inside that container:

```rust,ignore
make_aggregate!(
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account {
        overdraft: Option<Overdraft>,
        #[collection] cards: Vec<Card { limit: Money }>,
    }
);
// pub overdraft: Option<AccountAggregateOverdraft>,
// pub cards: Vec<AccountAggregateCards>,
```

- Without `#[collection]`, the part is a single value, such as an `Option`.
- With `#[collection]`, the part holds many values. The container must implement `FromIterator`:
  `Vec`, `HashSet`, or any type of yours that does.

Containers cannot be nested, and a `#[rootless]` part cannot be a collection.

### Derives

Every wrapper derives `Debug` and `Default`. `#[derive(...)]` on a part derives more traits on its
wrapper and on the wrappers inside it, so its domain types must implement them too. A `HashSet` of
parts needs `#[derive(PartialEq, Eq, Hash)]`:

```rust,ignore
make_aggregate!(
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account {
        #[collection] #[derive(PartialEq, Eq, Hash)] tags: HashSet<Tag>,
    }
);
```

### Renaming an aggregate

`#[alias = Name]` names the aggregate `NameAggregate` instead of `<root>Aggregate`. The nested
wrappers and `NAME` follow it. `#[alias(exact = Name)]` names the aggregate `Name`, as written:

```rust,ignore
make_aggregate!(
    #[alias = Wallet]
    #[emits = AccountEvent]
    #[fails = AccountError]
    Account { balance: Balance }
);
// pub struct WalletAggregate { pub root: Account, pub balance: WalletAggregateBalance }
```

This is how the same root can be the root of several aggregates.

## Building an aggregate

`<Aggregate>::builder()` returns a builder with one setter per part. Anything not set keeps its default
value, and `build()` never fails:

```rust,ignore
let aggregate = AccountAggregate::builder()
    .root(Account { id: 1, owner: "Ada".into() })
    .balance(Balance { cents: 0 })
    .card(card, |c| c.limit(limit))
    .limits(|l| l.daily(daily).monthly(monthly))
    .overdraft(overdraft)
    .cards(|| vec![card1, card2])
    .build();
```

- A part takes its domain value: `.balance(balance)`.
- A part with parts of its own also takes a closure that sets them: `.card(card, |c| c.limit(limit))`.
- A `#[rootless]` part takes only that closure: `.limits(|l| l.daily(daily))`.
- An `Option` part takes the value inside it: `.overdraft(overdraft)`.
- A `#[collection]` part takes a closure returning the domain values: `.cards(|| vec![card1, card2])`.
  The parts of each element keep their default values; set them through field access afterwards.

`#[no_builder]` before the root turns builders off for that aggregate. Its nested wrappers can still
be made from their domain type with `From`.

## Aggregate functions

The behaviour of an aggregate is written in an impl block marked with [`#[aggregate_functions]`]. These
functions are the only place where the aggregate changes:

```rust,ignore
use croisiere::domain::aggregate_functions;

#[aggregate_functions]
impl AccountAggregate {
    pub fn open(id: u32, owner: String) -> CreationResult {
        if owner.is_empty() {
            return Err(AccountError::EmptyOwner);
        }
        let events = vec![AccountEvent::Opened { id, owner: owner.clone() }];
        let aggregate = Self::builder().root(Account { id, owner }).build();
        Ok((aggregate, events))
    }

    pub fn deposit(&mut self, cents: i64) -> ActionResult {
        Self::check_amount(cents)?;
        self.balance.root.cents += cents;
        Ok(vec![AccountEvent::Deposited { id: self.root.id, cents }])
    }

    fn check_amount(cents: i64) -> CheckResult {
        if cents <= 0 {
            return Err(AccountError::InvalidAmount);
        }
        Ok(())
    }
}
```

The attribute gives the impl block result types built from the aggregate's events and errors:

| Type                 | Result of                                    | On success                       |
|----------------------|----------------------------------------------|----------------------------------|
| `CreationResult`     | creating an aggregate                        | the aggregate and its events     |
| `ActionResult`       | an action on an existing aggregate           | the emitted events               |
| `CheckResult`        | a check                                      | nothing                          |
| `AggregateResult<T>` | anything else                                | a `T`                            |

All of them fail with the aggregate's error.

The impl block is placed in a module named after the aggregate, `account_aggregate`, which is where
these types live. Functions called from outside the aggregate must therefore be `pub`.

Since the aggregate owns its parts, changing a field in place, as `deposit` does, is the same as
replacing the whole value: no one else holds a reference to it that could observe the change.

[`Aggregate`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.Aggregate.html
[`Aggregate::Event`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.Aggregate.html#associatedtype.Event
[`Aggregate::Error`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.Aggregate.html#associatedtype.Error
[`make_aggregate!`]: https://docs.rs/croisiere/latest/croisiere/domain/macro.make_aggregate.html
[`#[aggregate_functions]`]: https://docs.rs/croisiere/latest/croisiere/domain/attr.aggregate_functions.html
