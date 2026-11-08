/// Request to change the state of the application, as in CQRS.
///
/// A command typically loads aggregates through the repositories of a
/// [`UnitOfWork`](crate::application::UnitOfWork), runs aggregate functions on
/// them and registers the resulting snapshots to be persisted.
pub trait Command {
    /// What the command needs to run, e.g. a unit of work or repositories.
    type Context;
    /// Value returned on success.
    type Response;
    /// Error returned on failure.
    type Error;

    /// Name of the command, e.g. for logging or dispatching.
    fn name(&self) -> &'static str;
    /// Runs the command with `context`.
    fn handle(&self, context: Self::Context) -> Result<Self::Response, Self::Error>;
}

/// Request to read the state of the application without changing it, as in
/// CQRS.
pub trait Query {
    /// What the query needs to run, e.g. read repositories.
    type Context;
    /// Value returned on success.
    type Response;
    /// Error returned on failure.
    type Error;

    /// Name of the query, e.g. for logging or dispatching.
    fn name(&self) -> &'static str;
    /// Runs the query with `context`.
    fn handle(&self, context: Self::Context) -> Result<Self::Response, Self::Error>;
}
