# Domain: Value Objects

A value object is a piece of the domain that is defined only by its values. It has no identity: two
value objects holding the same values are the same value object. A `Balance` of 10 € is
interchangeable with any other `Balance` of 10 €, while two `Account`s with the same owner are still
two different accounts.

In `croisiere`, value objects are described by the [`ValueObject`] trait.

## Declaring a value object

Derive [`ValueObject`] on a struct or an enum, alongside its supertraits:

```rust,ignore
use croisiere::domain::ValueObject;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct Balance {
    pub cents: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub enum Currency {
    #[default]
    Euro,
    Dollar,
}
```

The derive only implements [`ValueObject`] itself. The supertraits have to be derived (or
implemented) by you:

| Trait             | Why                                                               |
| ----------------- | ----------------------------------------------------------------- |
| `PartialEq`, `Eq` | Value objects are compared by their values, not by an identity.   |
| `Hash`            | Equal values hash the same, so value objects can be used as keys. |
| `Clone`           | A value can be freely copied around without changing its meaning. |
| `Default`         | Aggregates and their builders start from a default state.         |
| `Debug`           | Every domain type can be inspected.                               |

Forgetting one of them is a compile error.

### `NAME`

[`ValueObject`] carries a single constant, `NAME`, which the derive sets to the type's identifier:

```rust,ignore
assert_eq!(Balance::NAME, "Balance");
```

Generic parameters are not part of the name: `Amount<T>` is named `"Amount"`.

## Rules

A value object **can contain other value objects**:

```rust,ignore
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct Price {
    pub amount: Money,
    pub currency: Currency,
}
```

Be thoughtful about which value objects you embed within another. An embedded value object becomes
part of the outer one's value: it is compared, hashed and replaced along with it. If two values
change independently, or only make sense together in some parts of your domain, link them in an
[aggregate](./aggregates.md) instead.

Thankfully, for value objects this choice is often straightforward: a `Price` without its `Currency`
means nothing, so `Currency` belongs inside it. For [entities](./entities.md), it is less so.

A value object **cannot contain [entities](./entities.md)**: it is defined only by its values, and an
entity inside it would bring an identity with it.

## Building value objects

> [!TIP]
> It is recommended to give value objects with more than a couple of fields a builder. You can write one
> by hand or use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).

> [!TIP]
> **Validate values when constructing the value object.** Put rules that depend only on its
> values in the value object itself. For example, a name that must not be empty should reject
> an empty string in its `TryFrom<String>` implementation, rather than relying on an aggregate
> to check it. Use `TryFrom` instead of `From` when conversion can fail, and keep fields private
> so callers cannot bypass validation or mutate a valid value into an invalid one. Apply the
> same validation when restoring values from snapshots. Since `ValueObject` requires `Default`,
> ensure that the default also satisfies its invariants.

> [!TIP]
> **Consider private fields with read-only getters.** You do not have to make every field `pub`.
> Getters let callers inspect a value while keeping its representation and mutation under your
> control. Writing them by hand can be repetitive; libraries such as `getset` can reduce that work.
> Review generated getters, though: a getter that borrows a field may expose its exact storage type
> instead of the most useful view. For a `String` field, a handwritten getter returning `&str`
> usually offers a more flexible interface than `&String`, without cloning or allocating.

[`ValueObject`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.ValueObject.html
