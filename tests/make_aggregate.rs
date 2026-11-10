//! What `make_aggregate!` generates: wrapper names and fields, the `Aggregate`
//! impl, aliases, rootless nodes, containers, derives and builders.
//!
//! Most checks are static: a test compiles only if the generated types have
//! the expected names, fields and impls.

use croisiere::domain::Aggregate;

/// Compiles only if `A` is an aggregate with these event and error types.
fn assert_aggregate_types<A: Aggregate<Event = E, Error = F>, E, F>() {}

#[derive(Debug)]
pub struct Event;

#[derive(Debug)]
pub struct Error;

/// Domain types: anything a wrapper can hold. Every wrapper derives `Debug`
/// and `Default`, so they need them too; the extra traits are for the derive
/// tests.
macro_rules! domain_types {
    ($($name:ident),* $(,)?) => {
        $(
            // Some are only ever built through `Default`.
            #[allow(dead_code)]
            #[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
            pub struct $name(pub u32);
        )*
    };
}

// --- Naming ------------------------------------------------------------------

mod naming {
    use super::*;

    domain_types!(A, B, C, D, E, F);

    pub mod inner {
        domain_types!(G);
    }

    croisiere::domain::make_aggregate!(
        #[emits = Event]
        #[fails = Error]
        A {
            b: B,
            c: C { d: D },
            line_items: E,
            r#type: F,
            path: inner::G,
        }
    );

    #[test]
    fn wrappers_are_named_after_the_root_and_field_path() {
        let aggregate = AAggregate::default();

        let _: A = aggregate.root;
        let _: AAggregateB = aggregate.b;
        let _: AAggregateC = aggregate.c;
        let _: AAggregateCD = aggregate.c.d;
        let _: B = AAggregateB::default().root;
        let _: D = AAggregateCD::default().root;
    }

    #[test]
    fn field_names_become_pascal_case() {
        let aggregate = AAggregate::default();

        let _: AAggregateLineItems = aggregate.line_items;
        let _: AAggregateType = aggregate.r#type;
    }

    #[test]
    fn domain_types_can_be_paths() {
        let _: inner::G = AAggregate::default().path.root;
    }

    #[test]
    fn aggregate_name_and_types() {
        assert_eq!(AAggregate::NAME, "AAggregate");
        assert_aggregate_types::<AAggregate, Event, Error>();
    }

    #[test]
    fn nested_wrappers_convert_from_their_domain_type() {
        let b: AAggregateB = B(1).into();
        assert_eq!(b.root, B(1));

        // A wrapper with children keeps them at their defaults.
        let c: AAggregateC = C(2).into();
        assert_eq!(c.root, C(2));
        assert_eq!(c.d.root, D::default());
    }
}

// --- Aliases and several roots -----------------------------------------------

mod alias {
    use super::*;

    domain_types!(Counter, Other, Plain, Data);

    pub struct OtherEvent;
    pub struct OtherError;

    // Several roots in one invocation, without commas between them.
    croisiere::domain::make_aggregate!(
        #[alias = Tally]
        #[emits = Event]
        #[fails = Error]
        Counter { data: Data }

        #[alias(exact = Exact)]
        #[emits = OtherEvent]
        #[fails = OtherError]
        Other { data: Data }

        #[emits = Event]
        #[fails = Error]
        Plain
    );

    #[test]
    fn alias_names_the_aggregate_and_its_wrappers() {
        let aggregate = TallyAggregate::default();
        let _: Counter = aggregate.root;
        let _: TallyAggregateData = aggregate.data;

        assert_eq!(TallyAggregate::NAME, "TallyAggregate");
    }

    #[test]
    fn exact_alias_names_the_aggregate_as_written() {
        let aggregate = Exact::default();
        let _: Other = aggregate.root;
        let _: ExactData = aggregate.data;

        assert_eq!(Exact::NAME, "Exact");
    }

    #[test]
    fn each_root_gets_its_own_types() {
        assert_aggregate_types::<TallyAggregate, Event, Error>();
        assert_aggregate_types::<Exact, OtherEvent, OtherError>();
        assert_aggregate_types::<PlainAggregate, Event, Error>();
    }

    #[test]
    fn root_without_fields() {
        let _: Plain = PlainAggregate::default().root;
        assert_eq!(PlainAggregate::NAME, "PlainAggregate");
    }
}

// --- Rootless nodes ----------------------------------------------------------

mod rootless {
    use super::*;

    domain_types!(A, Code);

    croisiere::domain::make_aggregate!(
        #[emits = Event]
        #[fails = Error]
        A {
            #[rootless] meta: Meta { code: Code }
            #[rootless] empty: Empty
        }
    );

    #[test]
    fn rootless_wrappers_have_no_root_but_keep_their_fields() {
        // Destructuring checks that `root` is absent: an extra field would
        // make the pattern fail to compile.
        let AAggregateMeta { code } = AAggregate::default().meta;
        let _: AAggregateMetaCode = code;
        let AAggregateEmpty {} = AAggregate::default().empty;
    }

    #[test]
    fn builder_sets_rootless_fields_through_a_closure() {
        let aggregate = AAggregate::builder()
            .meta(|m| m.code(Code(7)))
            .build();

        assert_eq!(aggregate.meta.code.root, Code(7));
    }
}

// --- Containers and collections ----------------------------------------------

mod containers {
    use std::collections::HashSet;

    use super::*;

    domain_types!(A, B, C, D, E, F, T);

    croisiere::domain::make_aggregate!(
        #[emits = Event]
        #[fails = Error]
        A {
            maybe: Option<D>,
            maybe_nested: Option<E { f: F }>,
            #[collection] items: Vec<B { c: C }>,
            #[collection] #[derive(PartialEq, Eq, Hash)] tags: HashSet<T>,
        }
    );

    #[test]
    fn container_fields_hold_wrappers() {
        let aggregate = AAggregate::default();

        let _: Option<AAggregateMaybe> = aggregate.maybe;
        let _: Option<AAggregateMaybeNested> = aggregate.maybe_nested;
        let _: Vec<AAggregateItems> = aggregate.items;
        let _: HashSet<AAggregateTags> = aggregate.tags;
    }

    #[test]
    fn containers_default_to_empty() {
        let aggregate = AAggregate::builder().build();

        assert!(aggregate.maybe.is_none());
        assert!(aggregate.maybe_nested.is_none());
        assert!(aggregate.items.is_empty());
        assert!(aggregate.tags.is_empty());
    }

    #[test]
    fn option_takes_the_value() {
        let aggregate = AAggregate::builder().maybe(D(1)).build();

        assert_eq!(aggregate.maybe.unwrap().root, D(1));
    }

    #[test]
    fn option_with_children_takes_the_value_and_a_closure() {
        let aggregate = AAggregate::builder()
            .maybe_nested(E(1), |e| e.f(F(2)))
            .build();

        let nested = aggregate.maybe_nested.unwrap();
        assert_eq!(nested.root, E(1));
        assert_eq!(nested.f.root, F(2));
    }

    #[test]
    fn collection_takes_an_iterator_and_keeps_children_default() {
        let aggregate = AAggregate::builder()
            .items(|| vec![B(1), B(2)])
            .build();

        let roots: Vec<_> = aggregate.items.iter().map(|i| i.root.clone()).collect();
        assert_eq!(roots, vec![B(1), B(2)]);
        assert!(aggregate.items.iter().all(|i| i.c.root == C::default()));
    }

    #[test]
    fn collection_accepts_wrappers_too() {
        // `T: Into<Wrapper>` is also satisfied by the wrapper itself.
        let aggregate = AAggregate::builder()
            .items(|| [AAggregateItems::from(B(3))])
            .build();

        assert_eq!(aggregate.items[0].root, B(3));
    }

    #[test]
    fn hash_set_collection_deduplicates() {
        let aggregate = AAggregate::builder()
            .tags(|| vec![T(1), T(1), T(2)])
            .build();

        assert_eq!(aggregate.tags.len(), 2);
        assert!(aggregate.tags.contains(&AAggregateTags::from(T(1))));
        assert!(aggregate.tags.contains(&AAggregateTags::from(T(2))));
    }
}

// --- Derives -----------------------------------------------------------------

mod derives {
    use super::*;

    domain_types!(A, B, C, D, E);

    fn assert_clone_partial_eq<T: Clone + PartialEq>() {}
    fn assert_clone_eq_hash<T: Clone + Eq + std::hash::Hash>() {}

    croisiere::domain::make_aggregate!(
        #[emits = Event]
        #[fails = Error]
        A {
            #[derive(Clone, PartialEq)] b: B { c: C }
            // `Clone` is inherited and written again: it must be derived once.
            #[derive(Clone)] d: D { #[derive(Clone, Eq, PartialEq, Hash)] e: E }
        }
    );

    #[test]
    fn derives_apply_to_the_wrapper_and_its_children() {
        assert_clone_partial_eq::<AAggregateB>();
        assert_clone_partial_eq::<AAggregateBC>();
    }

    #[test]
    fn derives_merge_with_inherited_ones() {
        assert_clone_eq_hash::<AAggregateDE>();
    }

    #[test]
    fn derived_traits_work() {
        let b: AAggregateB = B(1).into();
        assert_eq!(b.clone(), b);
        assert_ne!(AAggregateB::from(B(2)), b);
    }
}

// --- Builders ----------------------------------------------------------------

mod builders {
    use super::*;

    domain_types!(A, B, C, D);

    croisiere::domain::make_aggregate!(
        #[emits = Event]
        #[fails = Error]
        A {
            b: B,
            c: C { d: D },
        }
    );

    #[test]
    fn unset_fields_keep_their_defaults() {
        let aggregate = AAggregate::builder().build();

        assert_eq!(aggregate.root, A::default());
        assert_eq!(aggregate.b.root, B::default());
        assert_eq!(aggregate.c.root, C::default());
        assert_eq!(aggregate.c.d.root, D::default());
    }

    #[test]
    fn setters_set_their_field() {
        let aggregate = AAggregate::builder()
            .root(A(1))
            .b(B(2))
            .c(C(3), |c| c.d(D(4)))
            .build();

        assert_eq!(aggregate.root, A(1));
        assert_eq!(aggregate.b.root, B(2));
        assert_eq!(aggregate.c.root, C(3));
        assert_eq!(aggregate.c.d.root, D(4));
    }

    #[test]
    fn nested_closure_can_leave_children_unset() {
        let aggregate = AAggregate::builder().c(C(3), |c| c).build();

        assert_eq!(aggregate.c.root, C(3));
        assert_eq!(aggregate.c.d.root, D::default());
    }

    #[test]
    fn last_setter_call_wins() {
        let aggregate = AAggregate::builder().b(B(1)).b(B(2)).build();

        assert_eq!(aggregate.b.root, B(2));
    }
}

// --- #[no_builder] -----------------------------------------------------------

mod no_builder {
    use super::*;

    domain_types!(A, B, C);

    croisiere::domain::make_aggregate!(
        #[no_builder]
        #[emits = Event]
        #[fails = Error]
        A { b: B { c: C } }
    );

    #[test]
    fn wrappers_still_exist_and_convert_from_their_domain_type() {
        let b: AAggregateB = B(1).into();
        assert_eq!(b.root, B(1));
        assert_eq!(b.c.root, C::default());

        let aggregate = AAggregate {
            root: A(0),
            b,
        };
        assert_eq!(aggregate.b.root, B(1));
        assert_eq!(AAggregate::NAME, "AAggregate");
    }

    // That `builder()` and the builder types are absent can only be checked
    // with a compile-fail test.
}
