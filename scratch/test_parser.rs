#[cfg(test)]
mod tests {
    use crate::parser::parse;

    #[test]
    fn test_parse_simple_array_type() {
        let input = "func foo(arr: array[i32, 10]) {}";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }
}
