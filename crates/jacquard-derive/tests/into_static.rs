use jacquard_common::{CowStr, IntoStatic};
use jacquard_derive::IntoStatic;

#[derive(IntoStatic)]
struct SimpleStruct<'a> {
    name: CowStr<'a>,
    count: u32,
}

#[derive(IntoStatic)]
struct TupleStruct<'a>(CowStr<'a>, u32);

#[derive(IntoStatic)]
struct UnitStruct;

#[derive(IntoStatic)]
enum SimpleEnum<'a> {
    Variant1(CowStr<'a>),
    Variant2 { name: CowStr<'a>, value: u32 },
    Unit,
}

#[test]
fn test_struct_into_static() {
    let s = SimpleStruct {
        name: CowStr::from("test"),
        count: 42,
    };
    let static_s: SimpleStruct<'static> = s.into_static();
    assert_eq!(static_s.name.as_ref(), "test");
    assert_eq!(static_s.count, 42);
}

#[test]
fn test_tuple_struct_into_static() {
    let s = TupleStruct(CowStr::from("test"), 42);
    let static_s: TupleStruct<'static> = s.into_static();
    assert_eq!(static_s.0.as_ref(), "test");
    assert_eq!(static_s.1, 42);
}

#[test]
fn test_unit_struct_into_static() {
    let s = UnitStruct;
    let _static_s: UnitStruct = s.into_static();
}

#[test]
fn test_enum_into_static() {
    let e1 = SimpleEnum::Variant1(CowStr::from("test"));
    let static_e1: SimpleEnum<'static> = e1.into_static();
    match static_e1 {
        SimpleEnum::Variant1(name) => assert_eq!(name.as_ref(), "test"),
        _ => panic!("wrong variant"),
    }

    let e2 = SimpleEnum::Variant2 {
        name: CowStr::from("test"),
        value: 42,
    };
    let static_e2: SimpleEnum<'static> = e2.into_static();
    match static_e2 {
        SimpleEnum::Variant2 { name, value } => {
            assert_eq!(name.as_ref(), "test");
            assert_eq!(value, 42);
        }
        _ => panic!("wrong variant"),
    }

    let e3 = SimpleEnum::Unit;
    let static_e3: SimpleEnum<'static> = e3.into_static();
    match static_e3 {
        SimpleEnum::Unit => {}
        _ => panic!("wrong variant"),
    }
}
