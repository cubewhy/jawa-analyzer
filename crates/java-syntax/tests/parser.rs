use insta::assert_snapshot;
use java_syntax::parse;

macro_rules! parser_snapshot {
    ($name:ident, $input:expr $(,)?) => {
        #[test]
        fn $name() {
            let input = $input;
            let actual = parse(input).debug_dump();
            assert_snapshot!(stringify!($name), actual);
        }
    };
}

parser_snapshot!(
    parse_nested_wildcard_type_argument,
    r#"class A { Map<String, List<? extends Number>> xs; }"#
);

parser_snapshot!(parse_empty_class, r#"class A {}"#);

parser_snapshot!(parse_missing_semicolon, r#"class A { int x }"#);
