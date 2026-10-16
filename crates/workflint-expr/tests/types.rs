//! Ports of the type tests in the reference's `expr_type_test.go`, plus the cases where
//! workflint's types differ or the reference has no test.
//!
//! `TestExprTypeTypeOfJSONValue` and `TestExprTypePanicTypeOfJSONValue` are ported in
//! `from_json.rs`, through `fromJSON()`. The reference's `Merge` and `Assignable` take the
//! receiver first, so a port of `a.Merge(b)` is `a.merge(&b)` and a port of
//! `a.Assignable(b)` is `a.accepts(&b)`.

#![allow(clippy::panic)]

use workflint_expr::{ObjectTy, Rest, Ty};

fn array(elem: Ty) -> Ty {
    Ty::array(elem)
}

fn filtered(elem: Ty) -> Ty {
    Ty::filtered_array(elem)
}

/// `TestExprNewMapObjectType`.
mod new_map_object_type {
    use super::*;

    #[test]
    fn map_object_has_no_props_and_is_not_strict() {
        let ty = Ty::map(Ty::String);
        let Ty::Object(object) = &ty else {
            panic!("{ty} is not an object");
        };
        assert!(object.props().is_empty());
        assert_eq!(object.rest(), &Rest::Map(Ty::String));
        assert!(!object.is_strict());
        assert!(!object.is_loose());
    }

    #[test]
    fn map_of_any_is_a_loose_object() {
        assert_eq!(Ty::map(Ty::Any), Ty::empty_loose());
    }
}

/// `TestExprObjectTypeSetStrict`. The reference changes the object in place with `Strict()`
/// and `Loose()`. workflint returns a new object and leaves the original unchanged.
mod object_type_set_strict {
    use super::*;

    #[test]
    fn with_rest_returns_a_changed_copy() {
        let loose = ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose);
        assert!(!loose.is_strict() && loose.is_loose());

        let strict = loose.with_rest(Rest::Strict);
        assert!(strict.is_strict() && !strict.is_loose());
        assert!(!loose.is_strict() && loose.is_loose());

        let loose_again = strict.with_rest(Rest::Loose);
        assert!(!loose_again.is_strict() && loose_again.is_loose());
        assert!(strict.is_strict() && !strict.is_loose());
    }
}

/// `TestExprAssignableSimple`: each type accepts itself, `any` accepts each type, and `null`
/// accepts no type but `null` and `any`.
mod assignable_simple {
    use super::*;

    fn check(ty: &Ty) {
        assert!(ty.accepts(ty), "{ty} does not accept itself");
        if !matches!(ty, Ty::Null | Ty::Any) {
            assert!(!Ty::Null.accepts(ty), "null accepts {ty}");
        }
        assert!(Ty::Any.accepts(ty), "any does not accept {ty}");
    }

    macro_rules! cases {
        ($($name:ident: $ty:expr,)*) => {$(
            #[test]
            fn $name() {
                check(&$ty);
            }
        )*};
    }

    cases! {
        any: Ty::Any,
        null: Ty::Null,
        number: Ty::Number,
        bool: Ty::Bool,
        string: Ty::String,
        loose_object: Ty::loose([("n", Ty::Number)]),
        strict_object: Ty::strict([("b", Ty::Bool)]),
        map_object: Ty::map(Ty::Null),
        array_: array(Ty::String),
    }
}

/// `TestExprAssignableObject`.
mod assignable_object {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $from:expr => $to:expr, $yes:expr;)*) => {$(
            #[test]
            fn $name() {
                let (from, to): (Ty, Ty) = ($from, $to);
                assert_eq!(to.accepts(&from), $yes, "{to} accepts {from}");
            }
        )*};
    }

    cases! {
        map_to_map: Ty::map(Ty::Number) => Ty::map(Ty::String), true;
        loose_to_map: Ty::empty_loose() => Ty::map(Ty::String), true;
        map_to_loose: Ty::map(Ty::String) => Ty::empty_loose(), true;
        strict_to_map:
            Ty::strict([("a", Ty::Number), ("b", Ty::String)]) => Ty::map(Ty::String), true;
        strict_with_null_to_map:
            Ty::strict([("a", Ty::Null)]) => Ty::map(Ty::String), false;
        map_to_strict:
            Ty::map(Ty::Number) => Ty::strict([("a", Ty::Any), ("b", Ty::String)]), true;
        map_to_strict_with_null:
            Ty::map(Ty::Number) => Ty::strict([("a", Ty::Null), ("b", Ty::String)]), false;
        strict_to_strict:
            Ty::strict([("a", Ty::Number)]) => Ty::strict([("a", Ty::String)]), true;
        strict_to_strict_with_other_prop:
            Ty::strict([("a", Ty::String)]) => Ty::strict([("b", Ty::String)]), false;
        strict_with_null_to_strict:
            Ty::strict([("a", Ty::Null)]) => Ty::strict([("a", Ty::String)]), false;
    }
}

/// `TestExprEqualTypes`: each type equals itself, `any` and `eq`, and does not equal `neq`.
mod equal_types {
    use super::*;

    fn check(ty: &Ty, neq: Option<Ty>, eq: Option<Ty>) {
        assert!(ty.equivalent(ty), "{ty} does not equal itself");
        if let Some(neq) = neq {
            assert!(!ty.equivalent(&neq), "{ty} equals {neq}");
        }
        if let Some(eq) = eq {
            assert!(ty.equivalent(&eq), "{ty} does not equal {eq}");
        }
        assert!(ty.equivalent(&Ty::Any), "{ty} does not equal any");
        assert!(Ty::Any.equivalent(ty), "any does not equal {ty}");
    }

    macro_rules! cases {
        ($($name:ident: $ty:expr, neq: $neq:expr, eq: $eq:expr;)*) => {$(
            #[test]
            fn $name() {
                check(&$ty, $neq, $eq);
            }
        )*};
    }

    cases! {
        null: Ty::Null, neq: Some(Ty::String), eq: None;
        number: Ty::Number, neq: Some(Ty::String), eq: None;
        bool: Ty::Bool, neq: Some(Ty::String), eq: None;
        string: Ty::String, neq: Some(Ty::Bool), eq: None;
        object: Ty::empty_loose(), neq: Some(array(Ty::Any)), eq: None;
        strict_props_object: Ty::empty_strict(), neq: Some(array(Ty::Any)), eq: None;
        nested_object:
            Ty::loose([("foo", Ty::loose([("bar", Ty::String)]))]),
            neq: Some(array(Ty::Any)), eq: None;
        nested_strict_props_object:
            Ty::strict([("foo", Ty::strict([("bar", Ty::String)]))]),
            neq: Some(array(Ty::Any)), eq: None;
        nested_object_prop_name:
            Ty::strict([("foo", Ty::String)]),
            neq: Some(Ty::strict([("bar", Ty::String)])), eq: None;
        nested_object_prop_type:
            Ty::strict([("foo", Ty::String)]),
            neq: Some(Ty::strict([("foo", Ty::Bool)])), eq: None;
        strict_props_object_and_loose_object:
            Ty::strict([("foo", Ty::Null)]),
            neq: None, eq: Some(Ty::loose([("foo", Ty::Null)]));
        loose_object_and_strict_props_object:
            Ty::loose([("foo", Ty::Null)]),
            neq: None, eq: Some(Ty::loose([("foo", Ty::Null)]));
        map_objects:
            Ty::map(Ty::Null),
            neq: Some(Ty::map(Ty::Number)), eq: Some(Ty::map(Ty::Null));
        map_object_equals_loose_object:
            Ty::map(Ty::String), neq: None, eq: Some(Ty::empty_loose());
        loose_object_equals_map_object:
            Ty::empty_loose(), neq: None, eq: Some(Ty::map(Ty::String));
        map_object_equals_strict_object:
            Ty::map(Ty::String),
            neq: Some(Ty::strict([("foo", Ty::Null)])),
            eq: Some(Ty::strict([("foo", Ty::String)]));
        map_object_equals_strict_object_including_any_prop:
            Ty::map(Ty::String),
            neq: Some(Ty::strict([("foo", Ty::Null), ("bar", Ty::Any)])),
            eq: Some(Ty::strict([("foo", Ty::String), ("bar", Ty::Any)]));
        strict_object_equals_map_object:
            Ty::strict([("foo", Ty::String)]),
            neq: Some(Ty::map(Ty::Null)), eq: Some(Ty::map(Ty::String));
        array_: array(Ty::String), neq: Some(Ty::empty_loose()), eq: None;
        array_element_type: array(Ty::String), neq: Some(array(Ty::Bool)), eq: None;
        nested_array:
            array(array(Ty::String)), neq: Some(array(array(Ty::Bool))), eq: None;
    }
}

/// `TestExprTypeStringize`.
mod stringize {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $ty:expr => $want:expr,)*) => {$(
            #[test]
            fn $name() {
                assert_eq!($ty.to_string(), $want);
            }
        )*};
    }

    cases! {
        any: Ty::Any => "any",
        null: Ty::Null => "null",
        number: Ty::Number => "number",
        bool: Ty::Bool => "bool",
        string: Ty::String => "string",
        empty_object: Ty::empty_loose() => "object",
        empty_strict_props_object: Ty::empty_strict() => "{}",
        strict_object: Ty::strict([("foo", Ty::String)]) => "{foo: string}",
        loose_object: Ty::loose([("foo", Ty::String)]) => "object",
        multiple_props_object:
            Ty::strict([("foo", Ty::String), ("bar", Ty::Number)])
            => "{bar: number; foo: string}",
        nested_objects:
            Ty::strict([
                ("foo", Ty::String),
                ("nested", Ty::strict([("foo", Ty::String), ("bar", Ty::Number)])),
            ])
            => "{foo: string; nested: {bar: number; foo: string}}",
        array_: array(Ty::Any) => "array<any>",
        nested_arrays: array(filtered(Ty::Bool)) => "array<array<bool>>",
        array_nested_in_object:
            Ty::strict([("foo", array(Ty::strict([("bar", array(Ty::String))])))])
            => "{foo: array<{bar: array<string>}>}",
        map_object: Ty::map(Ty::Number) => "{string => number}",
    }

    #[test]
    fn keys_print_in_byte_order() {
        let ty = Ty::strict([
            ("b", Ty::Null),
            ("_", Ty::Null),
            ("B", Ty::Null),
            ("a", Ty::Null),
        ]);
        assert_eq!(ty.to_string(), "{B: null; _: null; a: null; b: null}");
    }
}

/// `TestExprTypeMergeSimple`.
mod merge_simple {
    use super::*;

    fn check(ty: &Ty) {
        assert_eq!(ty.merge(&Ty::Any), Ty::Any, "{ty} merged with any");
        assert_eq!(Ty::Any.merge(ty), Ty::Any, "any merged with {ty}");

        // null is compatible with null, so the reference uses string instead.
        let incompatible = if *ty == Ty::Null {
            Ty::String
        } else {
            Ty::Null
        };
        assert_eq!(
            ty.merge(&incompatible),
            Ty::Any,
            "{ty} merged with {incompatible}"
        );

        assert_eq!(&ty.merge(ty), ty, "{ty} merged with itself");
    }

    macro_rules! cases {
        ($($name:ident: $ty:expr,)*) => {$(
            #[test]
            fn $name() {
                check(&$ty);
            }
        )*};
    }

    cases! {
        any: Ty::Any,
        null: Ty::Null,
        number: Ty::Number,
        bool: Ty::Bool,
        string: Ty::String,
        empty_object: Ty::empty_loose(),
        empty_strict_object: Ty::empty_strict(),
        map_object: Ty::map(Ty::Null),
        array_: array(Ty::String),
    }
}

/// `TestExprTypeMergeComplicated`. Each case merges `ty` into `with`, as the reference does.
mod merge_complicated {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $ty:expr, with: $with:expr => $want:expr;)*) => {$(
            #[test]
            fn $name() {
                let (ty, with, want): (Ty, Ty, Ty) = ($ty, $with, $want);
                let have = with.merge(&ty);
                assert_eq!(have, want, "{ty} merged with {with} is {have}, not {want}");
            }
        )*};
    }

    cases! {
        number_merges_with_string: Ty::Number, with: Ty::String => Ty::String;
        string_is_merged_by_number: Ty::String, with: Ty::Number => Ty::String;
        bool_merges_with_string: Ty::Bool, with: Ty::String => Ty::String;
        string_is_merged_by_bool: Ty::String, with: Ty::Bool => Ty::String;
        object_props:
            Ty::loose([("foo", Ty::Number)]), with: Ty::loose([("bar", Ty::String)])
            => Ty::loose([("foo", Ty::Number), ("bar", Ty::String)]);
        loose_object_with_strict_object:
            Ty::loose([("foo", Ty::Number)]), with: Ty::strict([("bar", Ty::String)])
            => Ty::loose([("foo", Ty::Number), ("bar", Ty::String)]);
        strict_object_with_strict_object:
            Ty::strict([("foo", Ty::Number)]), with: Ty::strict([("bar", Ty::String)])
            => Ty::strict([("foo", Ty::Number), ("bar", Ty::String)]);
        compatible_prop:
            Ty::loose([("foo", Ty::Number)]), with: Ty::loose([("foo", Ty::String)])
            => Ty::loose([("foo", Ty::String)]);
        any_prop_with_prop:
            Ty::loose([("foo", Ty::Any)]), with: Ty::loose([("foo", Ty::String)])
            => Ty::loose([("foo", Ty::Any)]);
        prop_with_any_prop:
            Ty::loose([("foo", Ty::String)]), with: Ty::loose([("foo", Ty::Any)])
            => Ty::loose([("foo", Ty::Any)]);
        incompatible_prop:
            Ty::loose([("foo", Ty::Null)]), with: Ty::loose([("foo", Ty::String)])
            => Ty::loose([("foo", Ty::Any)]);
        compatible_array_element:
            array(Ty::Number), with: array(Ty::String) => array(Ty::String);
        incompatible_array_element:
            array(Ty::Null), with: array(Ty::String) => array(Ty::Any);
        any_array_element_with_element:
            array(Ty::Any), with: array(Ty::String) => array(Ty::Any);
        array_element_with_any_element:
            array(Ty::String), with: array(Ty::Any) => array(Ty::Any);
        array_with_array_deref:
            array(Ty::String), with: filtered(Ty::String) => array(Ty::String);
        array_deref_with_array:
            filtered(Ty::String), with: array(Ty::String) => array(Ty::String);
        array_deref_with_array_deref:
            filtered(Ty::String), with: filtered(Ty::String) => array(Ty::String);
        object_no_prop_at_left_hand_side:
            Ty::empty_loose(), with: Ty::loose([("foo", Ty::String)])
            => Ty::loose([("foo", Ty::String)]);
        object_no_prop_at_right_hand_side:
            Ty::loose([("foo", Ty::String)]), with: Ty::empty_loose()
            => Ty::loose([("foo", Ty::String)]);
        any_elem_array_at_left_hand_side:
            array(Ty::Any), with: array(Ty::String) => array(Ty::Any);
        any_elem_array_at_right_hand_side:
            array(Ty::String), with: array(Ty::Any) => array(Ty::Any);
        nested_array:
            array(array(Ty::Number)), with: array(array(Ty::String))
            => array(array(Ty::String));
        nested_objects:
            Ty::loose([
                ("foo", Ty::loose([("foo", Ty::Number), ("piyo", Ty::Number)])),
                ("aaa", Ty::Number),
                ("ccc", Ty::Number),
            ]),
            with: Ty::loose([
                ("foo", Ty::loose([("bar", Ty::String), ("piyo", Ty::String)])),
                ("bbb", Ty::String),
                ("ccc", Ty::String),
            ])
            => Ty::loose([
                (
                    "foo",
                    Ty::loose([("foo", Ty::Number), ("bar", Ty::String), ("piyo", Ty::String)]),
                ),
                ("aaa", Ty::Number),
                ("bbb", Ty::String),
                ("ccc", Ty::String),
            ]);
        map_object_with_compatible_map_object:
            Ty::map(Ty::Number), with: Ty::map(Ty::String) => Ty::map(Ty::String);
        map_object_with_incompatible_map_object:
            Ty::map(Ty::Number), with: Ty::map(Ty::Null) => Ty::empty_loose();
        map_object_with_compatible_object:
            Ty::map(Ty::Number), with: Ty::loose([("foo", Ty::Number)])
            => Ty::loose([("foo", Ty::Number)]);
        map_object_with_incompatible_object:
            Ty::map(Ty::Number), with: Ty::loose([("foo", Ty::Bool)])
            => Ty::loose([("foo", Ty::Bool)]);
    }

    /// The reference adds the new props of the merged object to the element type in Go map
    /// order, which is random, and the result depends on that order. workflint adds them in
    /// byte order of their names.
    #[test]
    fn new_props_merge_into_the_element_type_in_byte_order() {
        let map = Ty::map(Ty::Number);
        let strict = Ty::strict([("b", Ty::Bool), ("a", Ty::String)]);
        // number + string is string, then string + bool is string. In the other order,
        // number + bool is any, and the result would be a loose object.
        let merged = map.merge(&strict);
        assert_eq!(merged.to_string(), "{string => string}");
        let Ty::Object(object) = &merged else {
            panic!("{merged} is not an object");
        };
        assert_eq!(object.rest(), &Rest::Map(Ty::String));
        assert_eq!(object.prop("a"), Some(&Ty::String));
        assert_eq!(object.prop("b"), Some(&Ty::Bool));
    }
}

/// `TestExprTypeMergeCreateNewInstance`: merging leaves both inputs unchanged.
mod merge_create_new_instance {
    use super::*;

    #[test]
    fn array_merge_leaves_the_inputs_unchanged() {
        let ty = array(Ty::Number);
        let other = array(Ty::String);
        let merged = ty.merge(&other);
        assert_ne!(merged, ty);
        assert_eq!(ty, array(Ty::Number));
        assert_eq!(other, array(Ty::String));
    }

    #[test]
    fn object_merge_leaves_the_inputs_unchanged() {
        let ty = Ty::loose([("foo", Ty::Number)]);
        let other = Ty::loose([("foo", Ty::String), ("bar", Ty::Bool)]);
        let merged = ty.merge(&other);
        assert_ne!(merged, ty);
        assert_eq!(ty, Ty::loose([("foo", Ty::Number)]));
        assert_eq!(other, Ty::loose([("foo", Ty::String), ("bar", Ty::Bool)]));
    }
}

/// `TestExprTypeDeepCopy`. The reference copies a type before it changes the copy in place.
/// workflint types cannot change, so a clone shares its parts, and building a changed type
/// from a clone leaves the original unchanged.
mod deep_copy {
    use super::*;

    #[test]
    fn unit_types_clone_equal() {
        for ty in [Ty::Any, Ty::Null, Ty::Bool, Ty::String, Ty::Number] {
            assert_eq!(ty.clone(), ty);
        }
    }

    #[test]
    fn changing_a_cloned_object_leaves_the_original_unchanged() {
        let nested = Ty::loose([("piyo", Ty::String)]);
        let ty = Ty::loose([("foo", Ty::Number), ("bar", nested.clone())]);

        let Ty::Object(copy) = ty.clone() else {
            panic!("{ty} is not an object");
        };
        assert_eq!(copy.prop("foo"), Some(&Ty::Number));
        assert_eq!(copy.prop("bar"), Some(&nested));
        assert_eq!(copy.rest(), &Rest::Loose);

        let changed = ObjectTy::new(
            [("foo", Ty::Bool), ("bar", Ty::loose([("piyo", Ty::Null)]))],
            Rest::Map(Ty::Number),
        );
        assert_ne!(Ty::from(changed), ty);
        assert_eq!(
            ty,
            Ty::loose([
                ("foo", Ty::Number),
                ("bar", Ty::loose([("piyo", Ty::String)]))
            ])
        );
        assert_eq!(nested, Ty::loose([("piyo", Ty::String)]));
    }

    #[test]
    fn changing_a_cloned_array_leaves_the_original_unchanged() {
        let nested = array(Ty::String);
        let ty = array(nested.clone());
        let Ty::Array(copy) = ty.clone() else {
            panic!("{ty} is not an array");
        };
        assert_eq!(copy.elem(), &nested);
        let changed = array(array(Ty::Null));
        assert_ne!(changed, ty);
        assert_eq!(ty, array(array(Ty::String)));
    }
}

/// Only `bool` and `any` are assignable to `bool` (`docs/differences.md#bool-assignability`).
/// The reference accepts every type.
mod bool_assignability {
    use super::*;

    #[test]
    fn bool_accepts_bool_and_any() {
        assert!(Ty::Bool.accepts(&Ty::Bool));
        assert!(Ty::Bool.accepts(&Ty::Any));
    }

    #[test]
    fn bool_rejects_every_other_type() {
        for ty in [
            Ty::Null,
            Ty::Number,
            Ty::String,
            Ty::empty_loose(),
            Ty::empty_strict(),
            Ty::map(Ty::Bool),
            array(Ty::Bool),
        ] {
            assert!(!Ty::Bool.accepts(&ty), "bool accepts {ty}");
        }
    }

    #[test]
    fn bool_props_reject_strings() {
        let to = Ty::strict([("flag", Ty::Bool)]);
        assert!(!to.accepts(&Ty::strict([("flag", Ty::String)])));
        assert!(!array(Ty::Bool).accepts(&array(Ty::Number)));
    }

    #[test]
    fn every_type_converts_to_bool() {
        for ty in [
            Ty::Any,
            Ty::Null,
            Ty::Number,
            Ty::Bool,
            Ty::String,
            Ty::empty_loose(),
            Ty::empty_strict(),
            Ty::map(Ty::String),
            array(Ty::Any),
            filtered(Ty::Any),
        ] {
            assert!(ty.converts_to_bool(), "{ty} does not convert to bool");
        }
    }
}

/// `.*` filters (`docs/differences.md#object-filter-mutation`). The reference marks the array
/// type of the receiver as filtered in place.
mod object_filter {
    use super::*;

    #[test]
    fn filtering_an_array_leaves_a_shared_type_unchanged() {
        let os = array(Ty::Number);
        let matrix = Ty::strict([("os", os.clone())]);

        let Ty::Array(receiver) = &os else {
            panic!("{os} is not an array");
        };
        let result = Ty::Array(receiver.to_filtered());

        let Ty::Array(result) = &result else {
            panic!("{result} is not an array");
        };
        assert!(result.is_filtered());
        assert_eq!(result.elem(), &Ty::Number);

        assert!(!receiver.is_filtered());
        let Ty::Object(matrix) = &matrix else {
            panic!("{matrix} is not an object");
        };
        assert_eq!(matrix.prop("os"), Some(&array(Ty::Number)));
    }

    #[test]
    fn filtered_flag_is_part_of_equality_but_not_of_the_printed_form() {
        assert_ne!(filtered(Ty::String), array(Ty::String));
        assert_eq!(
            filtered(Ty::String).to_string(),
            array(Ty::String).to_string()
        );
        assert!(filtered(Ty::String).equivalent(&array(Ty::String)));
    }
}

/// Converting a `workflint-data` type description.
mod from_type_desc {
    use super::*;
    use workflint_data::{GLOBAL_CONTEXTS, TypeDesc, global_context};

    /// The printed form of each type in the reference's `BuiltinGlobalVariableTypes`, from
    /// `BuiltinGlobalVariableTypes[name].String()` at commit `011a6d1`.
    const REFERENCE: &[(&str, &str)] = &[
        ("env", "{string => string}"),
        (
            "github",
            "{action: string; action_path: string; action_ref: string; action_repository: string; \
             action_status: string; actor: string; actor_id: string; api_url: string; \
             artifact_cache_size_limit: number; base_ref: string; env: string; event: object; \
             event_name: string; event_path: string; graphql_url: string; head_ref: string; \
             job: string; output: string; path: string; ref: string; ref_name: string; \
             ref_protected: bool; ref_type: string; repository: string; repository_id: string; \
             repository_owner: string; repository_owner_id: string; repository_visibility: string; \
             repositoryurl: string; retention_days: number; run_attempt: string; run_id: string; \
             run_number: string; secret_source: string; server_url: string; sha: string; \
             state: string; step_summary: string; token: string; triggering_actor: string; \
             workflow: string; workflow_ref: string; workflow_sha: string; workspace: string}",
        ),
        ("inputs", "{}"),
        (
            "job",
            "{check_run_id: number; container: {id: string; network: string}; \
             services: {string => {id: string; network: string; ports: {string => string}}}; \
             status: string}",
        ),
        ("matrix", "{}"),
        ("needs", "{}"),
        (
            "runner",
            "{arch: string; debug: string; environment: string; name: string; os: string; \
             temp: string; tool_cache: string}",
        ),
        ("secrets", "{string => string}"),
        ("steps", "{}"),
        ("strategy", "object"),
        ("vars", "{string => string}"),
    ];

    #[test]
    fn every_global_context_prints_as_in_the_reference() {
        let names: Vec<_> = GLOBAL_CONTEXTS.iter().map(|context| context.name).collect();
        let reference: Vec<_> = REFERENCE.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, reference);
        for (name, want) in REFERENCE {
            let Some(context) = global_context(name) else {
                panic!("no global context {name:?}");
            };
            assert_eq!(Ty::from(context).to_string(), *want, "{name}");
        }
    }

    #[test]
    fn unit_types_convert_to_unit_types() {
        assert_eq!(Ty::from(&TypeDesc::Any), Ty::Any);
        assert_eq!(Ty::from(&TypeDesc::Null), Ty::Null);
        assert_eq!(Ty::from(&TypeDesc::Number), Ty::Number);
        assert_eq!(Ty::from(&TypeDesc::Bool), Ty::Bool);
        assert_eq!(Ty::from(&TypeDesc::String), Ty::String);
    }

    #[test]
    fn arrays_convert_unfiltered() {
        assert_eq!(
            Ty::from(&TypeDesc::Array(&TypeDesc::String)),
            array(Ty::String)
        );
    }

    #[test]
    fn objects_keep_their_props_and_rest() {
        let Some(job) = global_context("job") else {
            panic!("no job context");
        };
        let want = Ty::strict([
            ("check_run_id", Ty::Number),
            (
                "container",
                Ty::strict([("id", Ty::String), ("network", Ty::String)]),
            ),
            (
                "services",
                Ty::map(Ty::strict([
                    ("id", Ty::String),
                    ("network", Ty::String),
                    ("ports", Ty::map(Ty::String)),
                ])),
            ),
            ("status", Ty::String),
        ]);
        assert_eq!(Ty::from(job), want);

        let Some(strategy) = global_context("strategy") else {
            panic!("no strategy context");
        };
        let Ty::Object(strategy) = Ty::from(strategy) else {
            panic!("strategy is not an object");
        };
        assert!(strategy.is_loose());
        assert!(!strategy.props().is_empty());
    }
}

#[test]
fn types_can_cross_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Ty>();
    assert_send_sync::<ObjectTy>();
}
