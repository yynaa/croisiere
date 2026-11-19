# Seaport: Infrastructure

The infrastructure connects the application's persistence traits to storage. The domain works with
aggregates, the application works with their snapshots, and the infrastructure decides how those
snapshots become stored rows and how they are read back.

`croisiere::infrastructure::persistence` provides DTO traits and repository adapters for that work.
A DTO (data transfer object) describes the storage representation; it can keep database identifiers,
column formats and other storage details out of the application snapshot.

The usual path through the layers is:

```text
aggregate <-> snapshot + version <-> DTO <-> storage
             application           infrastructure
```

The snapshot converts to and from its aggregate. Separately, the infrastructure converts between a
DTO and a `VersionedSnapshot<S>`, preserving the version so writes can check for concurrent changes.
One DTO can support more than one snapshot type.

## What the infrastructure provides

| Part | Role |
|------|------|
| [DTOs](./infrastructure/dtos.md) | Describe storage data, connections, errors and select/insert/update/delete operations. |
| [Repositories](./infrastructure/repositories.md) | Adapt those operations to the application's `ReadRepository` and `WriteRepository` traits. |
| [Units of work](./infrastructure/units_of_work.md) | Connect repositories to a transaction and commit registered changes together. |
| [In-memory storage](./infrastructure/in_memory.md) | Backend under development, with type-erased tables and an unfinished unit of work. |

The DTO adapters supply the read/write plumbing. The storage implementation must still define query
semantics, identifier handling, version checks and transaction boundaries. A successful adapter call
does not by itself mean that a transaction has committed.

## Current scope

The persistence API exports `InternalId`, `DTO`, `DTOSelect`, `DTOInsert`, `DTOUpdate`, `DTODelete`,
`DTORepository`, `DTOReadRepository`, `DTOWriteRepository` and `DTORepositoryError`. Import them from
`croisiere::infrastructure::persistence`; their source submodules are private.

The `in_memory` module is available with the `in-memory` feature, which is enabled by default.
It does not yet provide a complete storage backend: its unit of work still has `todo!()` in
`commit_and_rollback`. The hand-written storage and transaction examples in this guide explain how
to implement a backend; those example types are separate from the crate's in-memory types.
