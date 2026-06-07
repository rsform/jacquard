//! `atproto!` macro.

use crate::{DefaultStr, FromStaticStr, types::value::Data};

/// Hidden conversion hook used by the [`atproto!`] macro.
#[doc(hidden)]
pub trait AtprotoMacroLiteral {
    /// Convert this literal into default-backed AT Protocol data.
    fn into_atproto_data(self) -> Data;
}

impl AtprotoMacroLiteral for &'static str {
    fn into_atproto_data(self) -> Data {
        Data::String(crate::types::string::AtprotoStr::new(
            DefaultStr::from_static(self),
        ))
    }
}

macro_rules! impl_atproto_macro_integer_literal {
    ($($ty:ty),* $(,)?) => {
        $(
            impl AtprotoMacroLiteral for $ty {
                fn into_atproto_data(self) -> Data {
                    Data::Integer(i64::from(self))
                }
            }
        )*
    };
}

macro_rules! impl_atproto_macro_checked_integer_literal {
    ($($ty:ty),* $(,)?) => {
        $(
            impl AtprotoMacroLiteral for $ty {
                fn into_atproto_data(self) -> Data {
                    Data::Integer(self.try_into().expect("integer literal exceeds the AT Protocol i64 range"))
                }
            }
        )*
    };
}

impl_atproto_macro_integer_literal!(i8, i16, i32, u8, u16, u32);
impl_atproto_macro_checked_integer_literal!(i64, i128, isize, u64, u128, usize);

/// Construct a default-backed atproto [`Data`] value from a literal.
///
/// [`Data`]: crate::types::value::Data
///
/// ```
/// # use jacquard_common::atproto;
/// #
/// let value = atproto!({
///     "code": 200,
///     "success": true,
///     "payload": {
///         "features": [
///             "serde",
///             "json"
///         ]
///     }
/// });
/// ```
///
/// Variables or expressions can be interpolated into the ATProto literal. Any type
/// interpolated into an array element or object value must implement Serde's
/// `Serialize` trait, while any type interpolated into an object key must
/// convert into the default string backing. If the `Serialize` implementation
/// of the interpolated type decides to fail, or if the interpolated type
/// contains a map with non-string keys, the `atproto!` macro will panic.
///
/// ```
/// # use jacquard_common::atproto;
/// #
/// let code = 200;
/// let features = vec!["serde", "json"];
///
/// let value = atproto!({
///     "code": code,
///     "success": code == 200,
///     "payload": {
///         features[0]: features[1]
///     }
/// });
/// ```
///
/// Trailing commas are allowed inside both arrays and objects.
///
/// ```
/// # use jacquard_common::atproto;
/// #
/// let value = atproto!([
///     "notice",
///     "the",
///     "trailing",
///     "comma -->",
/// ]);
/// ```
#[macro_export(local_inner_macros)]
macro_rules! atproto {
    // Hide distracting implementation details from the generated rustdoc.
    ($($atproto:tt)+) => {
        atproto_internal!($($atproto)+)
    };
}

#[macro_export(local_inner_macros)]
#[doc(hidden)]
macro_rules! atproto_internal {
    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an array [...]. Produces a vec![...]
    // of the elements.
    //
    // Must be invoked as: atproto_internal!(@array [] $($tt)*)
    //////////////////////////////////////////////////////////////////////////

    // Done with trailing comma.
    (@array [$($elems:expr,)*]) => {
        atproto_internal_vec![$($elems,)*]
    };

    // Done without trailing comma.
    (@array [$($elems:expr),*]) => {
        atproto_internal_vec![$($elems),*]
    };

    // Next element is `null`.
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!(null)] $($rest)*)
    };

    // Next element is `true`.
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!(true)] $($rest)*)
    };

    // Next element is `false`.
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!(false)] $($rest)*)
    };

    // Next element is an array.
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!([$($array)*])] $($rest)*)
    };

    // Next element is a map.
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!({$($map)*})] $($rest)*)
    };

    // Next element is an expression followed by comma.
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!($next),] $($rest)*)
    };

    // Last element is an expression with no trailing comma.
    (@array [$($elems:expr,)*] $last:expr) => {
        atproto_internal!(@array [$($elems,)* atproto_internal!($last)])
    };

    // Comma after the most recent element.
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        atproto_internal!(@array [$($elems,)*] $($rest)*)
    };

    // Unexpected token after most recent element.
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        atproto_unexpected!($unexpected)
    };

    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an object {...}. Each entry is
    // inserted into the given map variable.
    //
    // Must be invoked as: atproto_internal!(@object $map () ($($tt)*) ($($tt)*))
    //
    // We require two copies of the input tokens so that we can match on one
    // copy and trigger errors on the other copy.
    //////////////////////////////////////////////////////////////////////////

    // Done.
    (@object $object:ident () () ()) => {};

    // Insert the current entry followed by trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(atproto_internal_key!($($key)+), $value);
        atproto_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Current entry followed by unexpected token.
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        atproto_unexpected!($unexpected);
    };

    // Insert the last entry without trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(atproto_internal_key!($($key)+), $value);
    };

    // Next value is `null`.
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!(null)) $($rest)*);
    };

    // Next value is `true`.
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!(true)) $($rest)*);
    };

    // Next value is `false`.
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!(false)) $($rest)*);
    };

    // Next value is an array.
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!([$($array)*])) $($rest)*);
    };

    // Next value is a map.
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!({$($map)*})) $($rest)*);
    };

    // Next value is an expression followed by comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!($value)) , $($rest)*);
    };

    // Last value is an expression with no trailing comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        atproto_internal!(@object $object [$($key)+] (atproto_internal!($value)));
    };

    // Missing value for last entry. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        // "unexpected end of macro invocation"
        atproto_internal!();
    };

    // Missing colon and value for last entry. Trigger a reasonable error
    // message.
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        // "unexpected end of macro invocation"
        atproto_internal!();
    };

    // Misplaced colon. Trigger a reasonable error message.
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `:`".
        atproto_unexpected!($colon);
    };

    // Found a comma inside a key. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `,`".
        atproto_unexpected!($comma);
    };

    // Key is fully parenthesized. This avoids clippy double_parens false
    // positives because the parenthesization may be necessary here.
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };

    // Munch a token into the current key.
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        atproto_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };

    //////////////////////////////////////////////////////////////////////////
    // The main implementation.
    //
    // Must be invoked as: atproto_internal!($($atproto)+)
    //////////////////////////////////////////////////////////////////////////

    (null) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Null
    };

    (true) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Boolean(true)
    };

    (false) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Boolean(false)
    };

    ([]) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Array($crate::types::value::Array(atproto_internal_vec![]))
    };

    ([ $($tt:tt)+ ]) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Array($crate::types::value::Array(atproto_internal!(@array [] $($tt)+)))
    };

    ({}) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Object($crate::types::value::Object(::std::collections::BTreeMap::new()))
    };

    ({ $($tt:tt)+ }) => {
        $crate::types::value::Data::<$crate::DefaultStr>::Object($crate::types::value::Object({
            let mut object = ::std::collections::BTreeMap::new();
            atproto_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        }))
    };

    // Literal values go through a helper so string literals can use static
    // storage while integer literals remain integers.
    ($literal:literal) => {
        $crate::macros::AtprotoMacroLiteral::into_atproto_data($literal)
    };

    // Any Serialize type: variables, struct literals, dynamic strings etc.
    // Must be below every other rule.
    ($other:expr) => {
        {
            $crate::types::value::Data::<$crate::DefaultStr>::from($other)
        }
    };
}

// The atproto_internal macro above cannot invoke vec directly because it uses
// local_inner_macros. A vec invocation there would resolve to $crate::vec.
// Instead invoke vec here outside of local_inner_macros.
#[macro_export]
#[doc(hidden)]
macro_rules! atproto_internal_vec {
    ($($content:tt)*) => {
        ::std::vec![$($content)*]
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! atproto_internal_key {
    ($key:literal) => {
        <$crate::DefaultStr as $crate::FromStaticStr>::from_static($key)
    };

    ($key:expr) => {
        ($key).into()
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! atproto_unexpected {
    () => {};
}

#[cfg(test)]
mod tests {
    use crate::{DefaultStr, types::value::Data};

    const LONG_KEY: &str = "a-static-key-that-is-longer-than-inline-capacity";
    const LONG_VALUE: &str = "a static string value that is longer than inline capacity";

    #[test]
    fn string_literals_use_static_default_backing() {
        let value = atproto!({
            "a-static-key-that-is-longer-than-inline-capacity":
                "a static string value that is longer than inline capacity"
        });

        let Data::Object(object) = value else {
            panic!("expected object");
        };
        let (key, value) = object.0.iter().next().expect("object has one field");

        assert_eq!(key.as_str(), LONG_KEY);
        assert!(!key.is_heap_allocated());

        let Data::String(string) = value else {
            panic!("expected string value");
        };
        assert_eq!(string.as_str(), LONG_VALUE);
        if let crate::types::string::AtprotoStr::String(backing) = string {
            assert!(!backing.is_heap_allocated());
        } else {
            panic!("test value should not be inferred as a richer atproto string type");
        }
    }

    #[test]
    fn macro_result_defaults_to_default_backing_without_context() {
        let value = atproto!(["hello", 200, true, null]);
        let _: Data<DefaultStr> = value;
    }
}
