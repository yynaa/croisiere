//! Application layer: commands and queries, event services, snapshots of
//! aggregates and how they are persisted.

mod cqrs;
mod event_bus;
mod jobs;
mod persistence;
mod snapshot;

pub use cqrs::Command;
pub use cqrs::Query;
pub use event_bus::EventBus;
pub use event_bus::EventBusReader;
pub use event_bus::EventBusSendError;
pub use event_bus::EventBusSender;
pub use jobs::EventService;
pub use jobs::Job;
pub use persistence::AbstractActionableSnapshot;
pub use persistence::ActionableSnapshot;
pub use persistence::Pagination;
pub use persistence::ReadRepository;
pub use persistence::ReadRepositoryBox;
pub use persistence::ReadRepositoryFactory;
pub use persistence::RepositoryDropped;
pub use persistence::SharedWriteRepository;
pub use persistence::SnapshotAction;
pub use persistence::UnitOfWork;
pub use persistence::UnitOfWorkFactory;
pub use persistence::VersionedSnapshot;
pub use persistence::WriteRepository;
pub use persistence::WriteRepositoryWeak;
pub use snapshot::EmptyFilter;
pub use snapshot::EmptySorter;
pub use snapshot::Filter;
pub use snapshot::Snapshot;
pub use snapshot::Sorter;
