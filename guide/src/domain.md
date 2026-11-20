# Deep Water: Domain

The domain layer expresses what your application means: which values are valid, which operations
are allowed, and what happens when they succeed. `croisiere` separates these responsibilities into
[value objects](./domain/value_objects.md), [entities](./domain/entities.md) and
[aggregates](./domain/aggregates.md), with [domain services](./domain/services.md) for business logic
that does not belong to a single aggregate. This chapter explains the choices behind that separation.

## Business meaning belongs to the domain

Business rules should hold regardless of how an operation reaches your application. Creating an
account through an HTTP API, a command-line tool or a background job should apply the same rules.
Those rules belong in the domain, where each entry point can use them.

Keep domain logic independent of database representations, transport formats and presentation.
The application coordinates requests; infrastructure handles storage and external services; the
domain decides whether a business operation is valid. Changing a persistence backend should not
change what constitutes a valid domain value or operation.

## Validation follows responsibility

A value object enforces rules about its own values. If a name must be nonempty, constructing that
name should fail for an empty string. An entity holds identity and domain values; it delegates field
validation to value objects rather than repeating their checks.

An aggregate constructs those validated values from simple inputs and enforces rules involving its
state. A name can decide whether its string is valid, but it cannot decide whether a counter may be
decremented: that requires the counter's current count. Keep each rule with the information needed
to enforce it, and propagate value object errors through aggregate errors.

## External data carries no proof of validity

Application snapshots, API payloads and presentation inputs carry data in simple types such as
strings, numbers and enums. Successfully reading or parsing them does not establish that they
satisfy the domain's rules.

Apply domain validation whenever these values enter or re-enter the domain, including when
restoring an aggregate from a snapshot. Database constraints can provide useful additional checks,
but they may be weaker than the domain's rules or reflect an earlier version of them. Revalidation
also allows the domain to reject corrupted stored values that violate its invariants.

## Domain identity and storage identity serve different purposes

An entity's identity expresses how the domain recognizes it over time. That identity may be a
meaningful value object, while a database uses a separate generated key to store the row.

Keep storage-only identifiers in the infrastructure representation. A database key should enter the
domain only when it also has domain meaning. The choice of a primary key should not dictate how
business operations identify an entity.

## Failures and events are explicit outcomes

Aggregate operations return errors when business rules reject an operation and events to describe
what happened when it succeeds. Callers can handle those outcomes without reconstructing business
meaning from database failures or changes to individual fields.

Returning an event does not persist or publish it. The application coordinates persistence and event
delivery, while infrastructure provides the mechanisms. Keep domain errors about business rules;
storage and transport failures belong to the layers that perform those operations.

> [!TIP]
> **Export a `DomainError` enum that contains all domain error types.** Keep focused errors on
> value objects and aggregates, and wrap them in a public enum at the domain boundary. This gives
> the application one domain error type while preserving the specific failures for inspection.
> With `thiserror`, `#[error(transparent)]` and `#[from]` let each variant forward the wrapped
> error's message and provide a conversion from its type. Add new domain error types to the enum
> as the domain grows; keep persistence and transport errors outside it.

> [!TIP]
> **Export a `DomainEvent` enum that contains all domain event types.** Let aggregates return their
> focused event types, then wrap them in a public enum at the domain boundary, just as with
> `DomainError`. Implement `From` for each event type so callers can convert it with `.into()`.
> The application can handle one domain event type and match its variants when it needs the
> specific event. Keep application-only events, such as notification requests, in the application
> layer rather than adding them to `DomainEvent`.

## Framework tradeoffs

`croisiere` uses derives and macros to reduce repetitive declarations and give domain types a
consistent structure. Those tools cannot decide your business rules. Deriving a trait or generating
a builder does not establish that every possible instance is valid for your domain.

In particular, domain traits require `Default`, and generated aggregate builders fill omitted parts
with defaults. Their `build()` method is infallible. This makes assembly convenient, but requires
care: value object defaults must satisfy their invariants, and assembling an aggregate does not run
its creation operation or check rules involving several parts.

Use explicit aggregate creation functions for new business objects and validate data when restoring
existing ones. Treat defaults and builders as construction tools, not substitutes for those checks.
