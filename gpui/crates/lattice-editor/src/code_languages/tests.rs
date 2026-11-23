use super::*;

/// The language `code` names, built as the registry builds it.
fn build(dir: &str) -> Language {
    let code = CODE
        .iter()
        .find(|code| {
            LanguageConfig::from_toml(code.config)
                .unwrap()
                .grammar
                .as_deref()
                .is_some_and(|grammar| grammar == dir)
        })
        .unwrap_or_else(|| panic!("no code language on the {dir} grammar"));
    let config = LanguageConfig::from_toml(code.config).unwrap();
    let name = config.name.clone();
    Language::new(config, Some(code.grammar.into()))
        .with_highlights_query(code.highlights)
        .unwrap_or_else(|error| panic!("{name}'s highlight query compiles: {error}"))
}

/// The capture the code theme resolves each styled run of `text` to, as
/// `(run, capture)`; adjacent runs in one style are one run.
fn captures(language: Language, text: &str) -> Vec<(String, String)> {
    let language = Arc::new(language);
    let names = [
        "keyword",
        "function",
        "type",
        "variable",
        "variable.special",
        "string",
        "comment",
        "comment.doc",
        "number",
        "constant",
        "operator",
        "property",
        "attribute",
        "label",
        "punctuation",
        "punctuation.bracket",
    ];
    let theme = theme::SyntaxTheme::new(
        names
            .iter()
            .map(|name| (name.to_string(), gpui::HighlightStyle::default())),
    );
    language.set_theme(&theme);
    language
        .highlight_text(text, 0..text.len())
        .filter_map(|(range, id)| {
            Some((
                text[range].to_string(),
                theme.get_capture_name(id)?.to_string(),
            ))
        })
        .collect()
}

/// Every `(token, capture)` is a styled run of `text` in `dir`'s language.
#[track_caller]
fn assert_styles(dir: &str, text: &str, expected: &[(&str, &str)]) {
    let captured = captures(build(dir), text);
    for (token, capture) in expected {
        assert!(
            captured
                .iter()
                .any(|(run, name)| run == token && name == capture),
            "{dir}: {token:?} should be {capture}: {captured:?}"
        );
    }
}

#[test]
fn every_query_compiles() {
    for code in &CODE {
        let config = LanguageConfig::from_toml(code.config).unwrap();
        let name = config.name.clone();
        Language::new(config, Some(code.grammar.into()))
            .with_highlights_query(code.highlights)
            .unwrap_or_else(|error| panic!("{name}'s highlight query compiles: {error}"));
    }
}

#[test]
fn python() {
    assert_styles(
        "python",
        "# note\n@cache\ndef area(self, r=2):\n    \"\"\"Doc.\"\"\"\n    return math.pi * r ** 2 if r else None\nclass Shape: pass\nprint(f\"{x}\", True)\n",
        &[
            ("# note", "comment"),
            ("@cache", "attribute"),
            ("def", "keyword"),
            ("area", "function"),
            ("self", "variable.special"),
            ("2", "number"),
            ("\"\"\"Doc.\"\"\"", "comment.doc"),
            ("return", "keyword"),
            ("pi", "property"),
            ("**", "operator"),
            ("None", "constant"),
            ("Shape", "type"),
            ("print", "function"),
            ("True", "number"),
        ],
    );
}

#[test]
fn typescript_and_tsx() {
    let text =
        "interface Page { title: string }\nexport function show(p: Page): number { return 1; }\n";
    let expected = [
        ("interface", "keyword"),
        ("Page", "type"),
        ("string", "type"),
        ("function", "keyword"),
        ("show", "function"),
        ("1", "number"),
    ];
    assert_styles("typescript", text, &expected);
    assert_styles("tsx", text, &expected);
    assert_styles(
        "tsx",
        "const view = <Card title=\"x\">hi</Card>;\n",
        &[
            ("Card", "type"),
            ("title", "attribute"),
            ("\"x\"", "string"),
        ],
    );
}

#[test]
fn json_yaml_toml() {
    assert_styles(
        "json",
        "{\"name\": \"lattice\", \"n\": 3, \"ok\": true, \"none\": null}",
        &[
            ("\"name\"", "property"),
            ("\"lattice\"", "string"),
            ("3", "number"),
            ("true", "number"),
            ("null", "constant"),
        ],
    );
    assert_styles(
        "yaml",
        "# ci\nname: build\nretries: 3\nbase: &base\n  on: true\n",
        &[
            ("# ci", "comment"),
            ("name", "property"),
            ("build", "string"),
            ("3", "number"),
            ("base", "label"),
            ("true", "number"),
        ],
    );
    assert_styles(
        "toml",
        "[package]\nname = \"lattice\" # app\nversion = 2\n",
        &[
            ("package", "type"),
            ("name", "property"),
            ("\"lattice\"", "string"),
            ("# app", "comment"),
            ("2", "number"),
        ],
    );
}

#[test]
fn shell() {
    assert_styles(
        "bash",
        "#!/bin/zsh\nbuild() {\n  local dir=\"$HOME/x\"\n  ls -la \"$dir\" && echo done\n}\n",
        &[
            ("#!/bin/zsh", "comment"),
            ("build", "function"),
            ("local", "keyword"),
            ("ls", "function"),
            ("-la", "constant"),
            ("&&", "operator"),
        ],
    );
}

#[test]
fn c_and_cpp() {
    let text = "#include <stdio.h>\n// hi\nint main(void) { return MAX + 1; }\n";
    let expected = [
        ("#include", "keyword"),
        ("<stdio.h>", "string"),
        ("// hi", "comment"),
        ("int", "type"),
        ("main", "function"),
        ("return", "keyword"),
        ("MAX", "constant"),
        ("1", "number"),
    ];
    assert_styles("c", text, &expected);
    assert_styles("cpp", text, &expected);
    assert_styles(
        "cpp",
        "namespace app { class View { public: auto draw() -> void; }; }\nauto v = std::make_unique<View>(nullptr);\n",
        &[
            ("namespace", "keyword"),
            ("class", "keyword"),
            ("View", "type"),
            ("public", "keyword"),
            ("draw", "function"),
            ("make_unique", "function"),
            ("nullptr", "constant"),
        ],
    );
}

#[test]
fn rust() {
    assert_styles(
        "rust",
        "/// Doc.\n#[derive(Debug)]\npub struct Page<'a> { title: &'a str }\nfn show(&self) -> u32 { println!(\"hi\"); self.count() + 1 }\n",
        &[
            ("/// Doc.\n", "comment.doc"),
            ("#", "attribute"),
            ("Debug", "type"),
            ("pub", "keyword"),
            ("Page", "type"),
            ("a", "label"),
            ("title", "property"),
            ("str", "type"),
            ("show", "function"),
            ("u32", "type"),
            ("println!", "function"),
            ("\"hi\"", "string"),
            ("count", "function"),
            ("1", "number"),
        ],
    );
}

#[test]
fn go_and_java() {
    assert_styles(
        "go",
        "package main\n// hi\nfunc (s *Server) Run(n int) error { fmt.Println(\"x\"); return nil }\n",
        &[
            ("package", "keyword"),
            ("// hi", "comment"),
            ("Server", "type"),
            ("Run", "function"),
            ("int", "type"),
            ("Println", "function"),
            ("\"x\"", "string"),
            ("nil", "constant"),
        ],
    );
    assert_styles(
        "java",
        "/** Doc. */\n@Override\npublic class Main { int run(String s) { return this.count + 1; } }\n",
        &[
            ("/** Doc. */", "comment.doc"),
            ("@Override", "attribute"),
            ("public", "keyword"),
            ("Main", "type"),
            ("int", "type"),
            ("run", "function"),
            ("String", "type"),
            ("this", "variable.special"),
            ("count", "property"),
            ("1", "number"),
        ],
    );
}

#[test]
fn r_and_lua() {
    assert_styles(
        "r",
        "# fit\narea <- function(r) { pi * r^2 }\nx <- c(1, NA, TRUE)\nlibrary(stats); stats::lm(y ~ x, data = d)\n",
        &[
            ("# fit", "comment"),
            ("area", "function"),
            ("function", "keyword"),
            ("<-", "operator"),
            ("c", "function"),
            ("1", "number"),
            ("NA", "constant"),
            ("TRUE", "number"),
            ("stats", "variable.special"),
            ("lm", "function"),
            ("data", "property"),
        ],
    );
    assert_styles(
        "lua",
        "-- hi\nlocal function area(r) return math.pi * r end\nprint(\"x\", nil, self)\n",
        &[
            ("-- hi", "comment"),
            ("local", "keyword"),
            ("area", "function"),
            ("pi", "property"),
            ("end", "keyword"),
            ("print", "function"),
            ("\"x\"", "string"),
            ("nil", "constant"),
        ],
    );
}

#[test]
fn make_and_dockerfile() {
    assert_styles(
        "make",
        "# build\nCC := clang\nall: main.o\n\t$(CC) -o app $@\nifdef DEBUG\nendif\n",
        &[
            ("# build", "comment"),
            ("CC", "variable"),
            (":=", "operator"),
            ("all", "function"),
            ("ifdef", "keyword"),
        ],
    );
    assert_styles(
        "containerfile",
        "# base\nFROM rust:1.80 AS build\nENV MODE=release\nRUN cargo build\nEXPOSE 8080\n",
        &[
            ("# base\n", "comment"),
            ("FROM", "keyword"),
            ("rust", "type"),
            ("AS", "keyword"),
            ("MODE", "property"),
            ("RUN", "keyword"),
            ("8080", "number"),
        ],
    );
}

/// A file's name or extension picks its language as the configs list them.
#[test]
fn files_are_matched_by_name_and_extension() {
    let matchers: Vec<(String, Vec<String>)> = CODE
        .iter()
        .map(|code| {
            let config = LanguageConfig::from_toml(code.config).unwrap();
            (
                config.name.to_string(),
                config
                    .matcher
                    .path_suffixes
                    .iter()
                    .map(|suffix| suffix.to_string())
                    .collect(),
            )
        })
        .collect();
    let claims = |suffix: &str| -> Vec<&str> {
        matchers
            .iter()
            .filter(|(_, suffixes)| suffixes.iter().any(|s| s == suffix))
            .map(|(name, _)| name.as_str())
            .collect()
    };
    for (suffix, name) in [
        ("py", "Python"),
        ("ts", "TypeScript"),
        ("tsx", "TSX"),
        ("json", "JSON"),
        ("yml", "YAML"),
        ("toml", "TOML"),
        ("zsh", "Shell Script"),
        (".zshrc", "Shell Script"),
        ("h", "C"),
        ("hpp", "C++"),
        ("rs", "Rust"),
        ("go", "Go"),
        ("java", "Java"),
        ("R", "R"),
        ("lua", "Lua"),
        ("Makefile", "Makefile"),
        ("Dockerfile", "Dockerfile"),
    ] {
        assert_eq!(claims(suffix), [name], "{suffix}");
    }
}
