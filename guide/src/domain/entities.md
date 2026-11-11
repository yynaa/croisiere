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
|-----------|-----------------------------------------------------------|
| `Default` | Aggregates and their builders start from a default state. |
| `Debug`   | Every domain type can be inspected.                       |

Forgetting one of them is a compile error.

Unlike value objects, entities do not need `PartialEq`, `Eq`, `Hash` or `Clone`: they are not
compared or copied by their values.

### `NAME`

[`Entity`] carries a single constant, `NAME`, which the derive sets to the type's identifier:

```rust,ignore
assert_eq!(Account::NAME, "Account");
```

Generic parameters are not part of the name: `Wrapper<T>` is named `"Wrapper"`.

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

It is recommended to give entities with more than a couple of fields a builder. You can write one by
hand or use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).

[`Entity`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.Entity.html
