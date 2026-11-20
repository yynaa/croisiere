# Domain: Entities

An entity is a piece of the domain that has an identity and a lifecycle. Two entities holding the same
values are still two different entities: two `Account`s with the same owner are two different
accounts, and an `Account` stays the same account when its owner changes.

In `croisiere`, entities are described by the [`Entity`] trait.

## Declaring an entity

Derive [`Entity`] on a struct or an enum, alongside its supertraits:

```rust,ignore
use croisiere::domain::Entity;

#[derive(Debug, Default, Entity)]
pub struct Account {
    pub id: u32,
    pub owner: String,
}
```

The derive only implements [`Entity`] itself. The supertraits have to be derived (or implemented) by
you:

| Trait     | Why                                                       |
| --------- | --------------------------------------------------------- |
| `Default` | Aggregates and their builders start from a default state. |
| `Debug`   | Every domain type can be inspected.                       |

Forgetting one of them is a compile error.

> [!NOTE]
> Unlike value objects, the `Entity` trait deliberately does not require `PartialEq`, `Eq`, `Hash`
> or `Clone`.
>
> Equality is ambiguous for entities: should two entities with the same domain identity but
> different field values be equal? Comparing only their identities says yes, while comparing all
> their fields says no. Neither interpretation suits every use, so the trait does not impose one.
> `Hash` is not required either, since hashing must agree with whichever equality semantics you choose.
>
> Cloning raises a related problem: it creates another entity instance with the same domain identity.
> Both copies can then change independently, leaving two representations of the same entity with
> different state. Omitting the `Clone` requirement makes duplication an explicit design decision.
> You can still implement these traits when your domain needs them, but choose and document their
> semantics rather than deriving them automatically.

> [!TIP]
> **Avoid implementing `Clone` for entities.** Keeping a single owned instance helps prevent
> independently modified copies with the same domain identity. When you need a copy of the data
> for persistence or display, use a [snapshot](../application/snapshots.md) instead.

### `NAME`

[`Entity`] carries a single constant, `NAME`, which the derive sets to the type's identifier:

```rust,ignore
assert_eq!(Account::NAME, "Account");
```

Generic parameters are not part of the name: `Wrapper<T>` is named `"Wrapper"`.

## Domain identity

An identity can be a meaningful domain value rather than a database key. The counter example
uses a name represented by a value object:

```rust,ignore
use croisiere::domain::{Entity, ValueObject};

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterName(pub String);

#[derive(Debug, Default, Entity)]
pub struct Counter {
    pub name: CounterName,
}
```

`CounterName` compares by value, while the entity uses that value as its domain identity.
A storage-only UUID can live in the [DTO](../infrastructure/dtos.md#domain-and-storage-identifiers),
without adding it to the entity or snapshot. The entity derive does not select an identity field,
allocate keys or enforce uniqueness; the domain and persistence implementation define those rules.

## Rules

An entity **can contain [value objects](./value_objects.md) and other entities**:

```rust,ignore
#[derive(Debug, Default, Entity)]
pub struct Order {
    pub id: u32,
    pub shipping_address: Address, // a value object
    pub lines: Vec<OrderLine>,     // entities
}
```

The opposite is not true: **value objects cannot contain entities**. A value object is defined only
by its values, and an entity inside it would bring an identity with it.

Be thoughtful about which value objects and entities you embed within an entity. An embedded entity
lives and changes along with the one containing it. Unlike for value objects, this choice is often not
straightforward: an `OrderLine` clearly belongs to its `Order`, but whether a `Customer` belongs
inside an `Account`, or lives on its own, depends on your domain. When two entities live
independently, link them in an [aggregate](./aggregates.md) instead.

## Building entities

> [!TIP]
> It is recommended to give entities with more than a couple of fields a builder. You can write one by
> hand or use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).

> [!TIP]
> **Delegate field validation to value objects.** Entities should not implement validation.
> If a field requires validation, represent it with a [value object](./value_objects.md)
> that enforces those rules when constructed. The entity can then hold a valid value without
> repeating its checks.

[`Entity`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.Entity.html
