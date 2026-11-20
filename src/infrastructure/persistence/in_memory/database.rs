//! Type-erased tables for the in-memory backend.

use std::{any::Any, collections::HashMap};

use crate::infrastructure::persistence::in_memory::InMemoryRepositoryError;

/// Named tables of owned rows, stored in insertion order.
///
/// Rows must be `Any + Clone + Send + Sync`. Operations downcast every row in the
/// requested table to their generic row type, so mixing types in a table
/// causes reads, updates and deletes with an incompatible type to fail.
///
/// Construct an empty database with [`Default::default`]. It does not provide
/// locking, transactions, identity or version checks itself.
///
/// Cloning copies every table and clones each row using its concrete `Clone`
/// implementation. A clone can be assigned back to restore a saved database.
/// Shared mutable state inside a row remains shared if its `Clone` does so.
#[derive(Clone, Default)]
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub struct InMemoryDatabase {
    tables: HashMap<String, InMemoryDatabaseTable>,
}

/// A table whose rows retain their concrete types through [`Any`].
#[derive(Clone)]
struct InMemoryDatabaseTable {
    rows: Vec<Box<dyn Row>>,
}

trait Row: Any + Send + Sync {
    fn clone_box(&self) -> Box<dyn Row>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<O: Any + Clone + Send + Sync> Row for O {
    fn clone_box(&self) -> Box<dyn Row> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Clone for Box<dyn Row> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

impl InMemoryDatabase {
    /// Appends a row, creating `table` if it does not already exist.
    ///
    /// No uniqueness or row-type check is performed at insertion time.
    pub fn insert<O: Any + Clone + Sync + Send>(&mut self, table: &str, row: O) {
        self.tables
            .entry(table.to_string())
            .or_insert_with(|| InMemoryDatabaseTable { rows: Vec::new() })
            .rows
            .push(Box::new(row));
    }

    /// Removes rows matching `predicate`, returning how many were removed.
    ///
    /// Remaining rows retain their order. A missing table or a row of a
    /// different type returns an error before any rows are removed. An empty
    /// existing table returns zero.
    pub fn delete<O: 'static>(
        &mut self,
        table: &str,
        mut predicate: impl FnMut(&O) -> bool,
    ) -> Result<usize, InMemoryRepositoryError> {
        let table = self
            .tables
            .get_mut(table)
            .ok_or(InMemoryRepositoryError::TableNotFound(table.to_string()))?;

        let matches = table
            .rows
            .iter()
            .map(|row| {
                row.as_ref()
                    .as_any()
                    .downcast_ref::<O>()
                    .map(&mut predicate)
                    .ok_or(InMemoryRepositoryError::DowncastFromAnyImpossible)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let original_len = table.rows.len();
        let mut index = 0;
        table.rows.retain(|_| {
            let keep = !matches[index];
            index += 1;
            keep
        });
        Ok(original_len - table.rows.len())
    }

    /// Applies `update` to matching rows, returning how many matched.
    ///
    /// All row types are checked before any predicate or update runs, so a
    /// type error leaves the table unchanged. A missing table is an error;
    /// an empty existing table returns zero. Callback panics are not rolled
    /// back.
    pub fn update<O: 'static>(
        &mut self,
        table: &str,
        mut predicate: impl FnMut(&O) -> bool,
        mut update: impl FnMut(&mut O),
    ) -> Result<usize, InMemoryRepositoryError> {
        let table = self
            .tables
            .get_mut(table)
            .ok_or(InMemoryRepositoryError::TableNotFound(table.to_string()))?;

        let rows = table
            .rows
            .iter_mut()
            .map(|row| {
                row.as_mut()
                    .as_any_mut()
                    .downcast_mut::<O>()
                    .ok_or(InMemoryRepositoryError::DowncastFromAnyImpossible)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut updated = 0;
        for row in rows {
            if predicate(row) {
                update(row);
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// Clones every row in `table` in insertion order.
    ///
    /// Returns an error for a missing table or a row of a different type,
    /// rather than a partial result. An empty existing table returns an empty
    /// vector. Filtering, sorting and pagination belong to the DTO selector.
    pub fn read_rows<O: Clone + 'static>(
        &self,
        table: &str,
    ) -> Result<Vec<O>, InMemoryRepositoryError> {
        let rows = self
            .tables
            .get(table)
            .ok_or(InMemoryRepositoryError::TableNotFound(table.to_string()))?;

        rows.rows
            .iter()
            .map(|row| {
                row.as_ref()
                    .as_any()
                    .downcast_ref::<O>()
                    .cloned()
                    .ok_or(InMemoryRepositoryError::DowncastFromAnyImpossible)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{AbstractActionableSnapshot, SnapshotAction, UnitOfWork},
        infrastructure::persistence::in_memory::InMemoryUnitOfWork,
    };
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex},
    };

    struct TestAction {
        action: SnapshotAction,
        run: Box<dyn FnMut() -> Result<(), InMemoryRepositoryError>>,
    }

    impl AbstractActionableSnapshot for TestAction {
        type Error = InMemoryRepositoryError;

        fn get_name(&self) -> &str {
            "test"
        }

        fn get_action(&self) -> &SnapshotAction {
            &self.action
        }

        fn save(&mut self) -> Result<(), Self::Error> {
            (self.run)()
        }

        fn delete(&mut self) -> Result<(), Self::Error> {
            (self.run)()
        }
    }

    fn database() -> InMemoryDatabase {
        InMemoryDatabase {
            tables: HashMap::new(),
        }
    }

    #[test]
    fn commit_runs_actions_in_order_and_consumes_them() {
        let db = Arc::new(Mutex::new(database()));
        let uow = InMemoryUnitOfWork::new((), Arc::downgrade(&db));
        let calls = Rc::new(RefCell::new(Vec::new()));
        for (id, action) in [
            (1, SnapshotAction::Save),
            (2, SnapshotAction::Delete),
            (3, SnapshotAction::None),
        ] {
            let db = Arc::clone(&db);
            let calls = Rc::clone(&calls);
            uow.register_action(Box::new(TestAction {
                action,
                run: Box::new(move || {
                    calls.borrow_mut().push(id);
                    db.lock().unwrap().insert("numbers", id);
                    Ok(())
                }),
            }));
        }

        uow.commit_and_rollback().unwrap();
        uow.commit_and_rollback().unwrap();
        assert_eq!(*calls.borrow(), vec![1, 2]);
        assert_eq!(
            db.lock().unwrap().read_rows::<i32>("numbers").unwrap(),
            vec![1, 2]
        );
    }

    #[test]
    fn commit_restores_all_changes_on_error_and_discards_remaining_actions() {
        let db = Arc::new(Mutex::new(database()));
        db.lock().unwrap().insert("names", String::from("original"));
        let uow = InMemoryUnitOfWork::new((), Arc::downgrade(&db));
        let calls = Rc::new(RefCell::new(Vec::new()));
        for id in 1..=3 {
            let db = Arc::clone(&db);
            let calls = Rc::clone(&calls);
            uow.register_action(Box::new(TestAction {
                action: SnapshotAction::Save,
                run: Box::new(move || {
                    calls.borrow_mut().push(id);
                    let mut db = db.lock().unwrap();
                    db.update("names", |_: &String| true, |name| name.push('!'))?;
                    db.insert("new", id);
                    if id == 2 {
                        db.delete("names", |_: &String| true)?;
                        return Err(InMemoryRepositoryError::TableNotFound("failure".into()));
                    }
                    Ok(())
                }),
            }));
        }

        assert!(
            matches!(uow.commit_and_rollback(), Err(InMemoryRepositoryError::TableNotFound(name)) if name == "failure")
        );
        uow.commit_and_rollback().unwrap();
        assert_eq!(*calls.borrow(), vec![1, 2]);
        let db = db.lock().unwrap();
        assert_eq!(db.read_rows::<String>("names").unwrap(), vec!["original"]);
        assert!(matches!(
            db.read_rows::<i32>("new"),
            Err(InMemoryRepositoryError::TableNotFound(_))
        ));
    }

    #[test]
    fn commit_reports_an_unavailable_database_before_running_actions() {
        let db = Arc::new(Mutex::new(database()));
        let uow = InMemoryUnitOfWork::new((), Arc::downgrade(&db));
        let calls = Rc::new(RefCell::new(0));
        let action_calls = Rc::clone(&calls);
        uow.register_action(Box::new(TestAction {
            action: SnapshotAction::Save,
            run: Box::new(move || {
                *action_calls.borrow_mut() += 1;
                Ok(())
            }),
        }));
        let poisoned = Arc::clone(&db);
        assert!(
            std::thread::spawn(move || {
                let _guard = poisoned.lock().unwrap();
                panic!("poison database");
            })
            .join()
            .is_err()
        );

        assert!(matches!(
            uow.commit_and_rollback(),
            Err(InMemoryRepositoryError::DatabasePoisoned)
        ));
        db.clear_poison();
        uow.commit_and_rollback().unwrap();
        assert_eq!(*calls.borrow(), 0);

        drop(db);
        assert!(matches!(
            uow.commit_and_rollback(),
            Err(InMemoryRepositoryError::DatabaseDropped)
        ));
    }

    #[test]
    fn clone_preserves_tables_types_and_empty_tables() {
        let mut original = database();
        original.insert("mixed", 7_i32);
        original.insert("mixed", String::from("text"));
        original.insert("empty", 1_i32);
        original.delete("empty", |_: &i32| true).unwrap();

        let cloned = original.clone().clone();
        let rows = &cloned.tables["mixed"].rows;
        assert_eq!(rows[0].as_ref().as_any().downcast_ref::<i32>(), Some(&7));
        assert_eq!(
            rows[1].as_ref().as_any().downcast_ref::<String>().unwrap(),
            "text"
        );
        assert!(cloned.read_rows::<i32>("empty").unwrap().is_empty());
        assert!(database().clone().tables.is_empty());
    }

    #[test]
    fn saved_clone_restores_database_after_a_failed_modification() {
        let mut db = database();
        db.insert("names", String::from("Alice"));
        db.insert("names", String::from("Bob"));
        let saved = db.clone();

        db.update("names", |_: &String| true, |name| name.push('!'))
            .unwrap();
        db.delete("names", |name: &String| name == "Bob!").unwrap();
        db.insert("names", String::from("Carol"));
        db.insert("new", 42_i32);

        assert_eq!(
            saved.read_rows::<String>("names").unwrap(),
            ["Alice", "Bob"]
        );
        assert_eq!(
            db.read_rows::<String>("names").unwrap(),
            ["Alice!", "Carol"]
        );
        assert!(db.update("names", |_: &i32| true, |_| {}).is_err());

        db = saved;
        assert_eq!(db.read_rows::<String>("names").unwrap(), ["Alice", "Bob"]);
        assert!(matches!(
            db.read_rows::<i32>("new"),
            Err(InMemoryRepositoryError::TableNotFound(_))
        ));
    }

    #[test]
    fn delete_removes_all_matches_and_preserves_order() {
        let mut db = database();
        for value in [1, 2, 3, 4] {
            db.insert("numbers", value);
        }

        assert_eq!(db.delete("numbers", |n: &i32| n % 2 == 0).unwrap(), 2);
        assert_eq!(db.read_rows::<i32>("numbers").unwrap(), vec![1, 3]);
        assert_eq!(db.delete("numbers", |_: &i32| false).unwrap(), 0);
        assert_eq!(db.delete("numbers", |_: &i32| true).unwrap(), 2);
        assert_eq!(db.delete("numbers", |_: &i32| true).unwrap(), 0);
    }

    #[test]
    fn update_changes_only_matches() {
        let mut db = database();
        for value in [1, 2, 3, 4] {
            db.insert("numbers", value);
        }

        assert_eq!(
            db.update("numbers", |n: &i32| n % 2 == 0, |n| *n *= 10)
                .unwrap(),
            2
        );
        assert_eq!(db.read_rows::<i32>("numbers").unwrap(), vec![1, 20, 3, 40]);
        assert_eq!(
            db.update("numbers", |_: &i32| false, |_| panic!("no matches"))
                .unwrap(),
            0
        );
    }

    #[test]
    fn type_errors_leave_rows_unchanged() {
        let mut db = database();
        db.insert("mixed", 1_i32);
        db.insert("mixed", String::from("text"));

        assert!(matches!(
            db.delete("mixed", |_: &i32| true),
            Err(InMemoryRepositoryError::DowncastFromAnyImpossible)
        ));
        assert!(matches!(
            db.update("mixed", |_: &i32| true, |n| *n = 99),
            Err(InMemoryRepositoryError::DowncastFromAnyImpossible)
        ));
        let rows = &db.tables["mixed"].rows;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].as_ref().as_any().downcast_ref::<i32>(), Some(&1));
        assert_eq!(
            rows[1].as_ref().as_any().downcast_ref::<String>().unwrap(),
            "text"
        );
    }

    #[test]
    fn missing_tables_return_errors() {
        let mut db = database();
        assert!(matches!(
            db.delete("missing", |_: &i32| true),
            Err(InMemoryRepositoryError::TableNotFound(name)) if name == "missing"
        ));
        assert!(matches!(
            db.update("missing", |_: &i32| true, |_| {}),
            Err(InMemoryRepositoryError::TableNotFound(name)) if name == "missing"
        ));
    }
}
