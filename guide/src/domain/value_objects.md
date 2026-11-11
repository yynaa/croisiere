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

| Trait                 | Why                                                                 |
|-----------------------|---------------------------------------------------------------------|
| `PartialEq`, `Eq`     | Value objects are compared by their values, not by an identity.     |
| `Hash`                | Equal values hash the same, so value objects can be used as keys.   |
| `Clone`               | A value can be freely copied around without changing its meaning.  |
| `Default`             | Aggregates and their builders start from a default state.          |
| `Debug`               | Every domain type can be inspected.                                 |

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

It is recommended to give value objects with more than a couple of fields a builder. You can write one
by hand or use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).

[`ValueObject`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.ValueObject.html
