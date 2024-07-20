pub(crate) enum IdentCase {
    UpperCamelCase,
    LowerCamelCase,
    UpperSnakeCase,
    LowerSnakeCase
}

pub(crate) fn change_case(input: &str, case: IdentCase) -> String {
    input.chars()
        .scan((true, true, false), |(first, prev_lowercase, split_queued), c| {
            let is_delimiter = c == ' ' || c == '-' || c == '_';
            let do_split = is_delimiter || *split_queued || (!*first && *prev_lowercase && c.is_uppercase());
            *split_queued = false;

            let out = match case {
                IdentCase::UpperCamelCase => if is_delimiter {
                    *split_queued = true;
                    String::new()
                } else if do_split || *first {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                },
                IdentCase::LowerCamelCase => if is_delimiter {
                    *split_queued = true;
                    String::new()
                } else if do_split {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                },
                IdentCase::UpperSnakeCase => if is_delimiter {
                    "_".to_owned()
                } else if do_split {
                    "_".to_owned() + &c.to_uppercase().to_string()
                } else {
                    c.to_uppercase().to_string()
                }
                IdentCase::LowerSnakeCase => if is_delimiter {
                    "_".to_owned()
                } else if do_split {
                    "_".to_owned() + &c.to_lowercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                }
            };
            *first = false;
            *prev_lowercase = c.is_lowercase();
            Some(out)
        })
        .collect()
}


#[cfg(test)]
mod tests {
    use crate::util::{change_case, IdentCase};

    #[test]
    fn test_change_case() {
        assert_eq!(change_case("test", IdentCase::UpperCamelCase), "Test");
        assert_eq!(change_case("test", IdentCase::LowerCamelCase), "test");
        assert_eq!(change_case("test", IdentCase::UpperSnakeCase), "TEST");
        assert_eq!(change_case("test", IdentCase::LowerSnakeCase), "test");

        assert_eq!(change_case("TestText", IdentCase::UpperCamelCase), "TestText");
        assert_eq!(change_case("TestText", IdentCase::LowerCamelCase), "testText");
        assert_eq!(change_case("TestText", IdentCase::UpperSnakeCase), "TEST_TEXT");
        assert_eq!(change_case("TestText", IdentCase::LowerSnakeCase), "test_text");

        assert_eq!(change_case("test_text", IdentCase::UpperCamelCase), "TestText");
        assert_eq!(change_case("test_text", IdentCase::LowerCamelCase), "testText");
        assert_eq!(change_case("test_text", IdentCase::UpperSnakeCase), "TEST_TEXT");
        assert_eq!(change_case("test_text", IdentCase::LowerSnakeCase), "test_text");
    }
}
