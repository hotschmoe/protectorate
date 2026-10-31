/// Declares structs with `ToJson` and `FromJson`. JSON keys are the field names (`r#type` is
/// `type`). A field whose JSON is null (`None`) is omitted; a missing key reads as null, so
/// `Option` fields may be absent and the rest are required, unless given a default with
/// `name: Type = expr`, used when the key is missing or null. Unknown keys are ignored.
#[macro_export]
macro_rules! record {
    ($(
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $( $(#[$fmeta:meta])* $fvis:vis $field:ident : $fty:ty $(= $default:expr)? ),* $(,)?
        }
    )*) => {$(
        $(#[$meta])*
        $vis struct $name {
            $( $(#[$fmeta])* $fvis $field: $fty, )*
        }

        impl $crate::ToJson for $name {
            fn to_json(&self) -> $crate::Value {
                #[allow(unused_mut)]
                let mut map = $crate::Map::new();
                $( $crate::__private::put(
                    &mut map,
                    $crate::__private::key(::core::stringify!($field)),
                    &self.$field,
                ); )*
                $crate::Value::Object(map)
            }
        }

        impl $crate::FromJson for $name {
            #[allow(unused_variables)]
            fn from_json(value: &$crate::Value) -> ::core::result::Result<Self, $crate::Error> {
                let map = $crate::__private::object(value)?;
                ::core::result::Result::Ok(Self {
                    $( $field: $crate::__pt_json_field!(map, $field, $fty $(, $default)?), )*
                })
            }
        }
    )*};
}

/// Declares fieldless enums that map to JSON strings, with `as_str`, `ALL`, `Display` and
/// `FromStr`.
#[macro_export]
macro_rules! string_enum {
    ($(
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $text:literal ),* $(,)?
        }
    )*) => {$(
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )*
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            pub fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => $text, )*
                }
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::Error;
            fn from_str(s: &str) -> ::core::result::Result<Self, $crate::Error> {
                match s {
                    $( $text => ::core::result::Result::Ok(Self::$variant), )*
                    _ => ::core::result::Result::Err($crate::__private::unknown(s, &[$($text),*])),
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl $crate::ToJson for $name {
            fn to_json(&self) -> $crate::Value {
                $crate::Value::from(self.as_str())
            }
        }

        impl $crate::FromJson for $name {
            fn from_json(value: &$crate::Value) -> ::core::result::Result<Self, $crate::Error> {
                match value.as_str() {
                    ::core::option::Option::Some(s) => s.parse(),
                    ::core::option::Option::None => {
                        ::core::result::Result::Err($crate::Error::new("expected string"))
                    }
                }
            }
        }
    )*};
}

/// Declares internally tagged enums: `{"type": "<tag>", ...fields}`. Each variant is
/// struct-like; fields follow the `record!` rules. A field must not be named `type`.
#[macro_export]
macro_rules! tagged_enum {
    ($(
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $tag:literal {
                $( $(#[$fmeta:meta])* $field:ident : $fty:ty $(= $default:expr)? ),* $(,)?
            } ),* $(,)?
        }
    )*) => {$(
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant { $( $(#[$fmeta])* $field: $fty, )* }, )*
        }

        impl $crate::ToJson for $name {
            fn to_json(&self) -> $crate::Value {
                match self {
                    $( Self::$variant { $($field),* } => {
                        #[allow(unused_mut)]
                        let mut map = $crate::__private::tagged($tag);
                        $( $crate::__private::put(
                            &mut map,
                            $crate::__private::key(::core::stringify!($field)),
                            $field,
                        ); )*
                        $crate::Value::Object(map)
                    } )*
                }
            }
        }

        impl $crate::FromJson for $name {
            #[allow(unused_variables)]
            fn from_json(value: &$crate::Value) -> ::core::result::Result<Self, $crate::Error> {
                let map = $crate::__private::object(value)?;
                match $crate::__private::tag(map)? {
                    $( $tag => ::core::result::Result::Ok(Self::$variant {
                        $( $field: $crate::__pt_json_field!(map, $field, $fty $(, $default)?), )*
                    }), )*
                    other => ::core::result::Result::Err(
                        $crate::__private::unknown(other, &[$($tag),*]).in_field("type"),
                    ),
                }
            }
        }
    )*};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __pt_json_field {
    ($map:ident, $field:ident, $fty:ty) => {
        $crate::__private::field::<$fty>($map, $crate::__private::key(::core::stringify!($field)))?
    };
    ($map:ident, $field:ident, $fty:ty, $default:expr) => {
        match $crate::__private::field_or::<$fty>(
            $map,
            $crate::__private::key(::core::stringify!($field)),
        )? {
            ::core::option::Option::Some(v) => v,
            ::core::option::Option::None => $default,
        }
    };
}

/// Builds an object `Value`: `obj! { "a" => 1, "b" => "x" }`. Values go through `Value::from`;
/// a repeated key replaces the earlier value in place.
#[macro_export]
macro_rules! obj {
    ($($key:expr => $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut map = $crate::Map::new();
        $( map.insert($key, $value); )*
        $crate::Value::Object(map)
    }};
}
