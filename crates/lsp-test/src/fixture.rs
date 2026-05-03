pub struct ParsedFixture<'a> {
    pub path: &'a str,
    pub content: &'a str,
}

pub fn parse_fixtures(fixture: &str) -> Vec<ParsedFixture<'_>> {
    fixture
        .split("//- ")
        .filter_map(|segment| {
            let segment = segment.trim_start();
            if segment.is_empty() {
                return None;
            }

            let mut parts = segment.splitn(2, '\n');
            let raw_path = parts.next()?.trim();

            if raw_path.is_empty() {
                return None;
            }

            let path = raw_path.trim_start_matches('/');
            let content = parts.next().unwrap_or("");

            Some(ParsedFixture { path, content })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_file() {
        let input = r#"
        //- /src/Main.java
        package com.example;
        "#;
        let res = parse_fixtures(input);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].path, "src/Main.java");
        assert!(res[0].content.contains("package com.example;"));
    }

    #[test]
    fn test_parse_multiple_files() {
        let input = r#"
//- a.txt
content a
//- b.txt
content b
"#;
        let res = parse_fixtures(input);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].path, "a.txt");
        assert_eq!(res[0].content.trim(), "content a");
        assert_eq!(res[1].path, "b.txt");
        assert_eq!(res[1].content.trim(), "content b");
    }

    #[test]
    fn test_empty_input() {
        let res = parse_fixtures("   \n  ");
        assert!(res.is_empty());
    }

    #[test]
    fn test_no_content() {
        let res = parse_fixtures("//- empty.txt");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].path, "empty.txt");
        assert_eq!(res[0].content, "");
    }
}
