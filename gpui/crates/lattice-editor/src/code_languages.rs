//! The code languages a project's other files open in: Python, JavaScript's
//! TypeScript and TSX, JSON, YAML, TOML, shell scripts, C, C++, Rust, Go,
//! Java, R, Lua, Makefiles and Dockerfiles, which the Tauri app highlights
//! through `@codemirror/language-data` (`loadTextLanguageExtensions`). Each is
//! a tree-sitter grammar with Lattice's own highlight query in the code
//! theme's names, as [`crate::languages`]' are.
//!
//! None of them is built at start-up: compiling their queries takes about
//! 65 ms together. They are registered in the registry every source buffer
//! resolves its injections through ([`crate::markdown::registry`]), which
//! builds one the first time a file or a Markdown fence names it, on the
//! background executor.

use std::{borrow::Cow, future::Future, path::Path, sync::Arc};

use futures::{FutureExt as _, StreamExt as _};
use gpui::{App, Entity};
use language::{
    Buffer, Language, LanguageConfig, LanguageQueries, LanguageRegistry, LoadedLanguage,
};
use tree_sitter_language::LanguageFn;

/// A code language: its config, highlight query and grammar.
pub(crate) struct Code {
    pub config: &'static str,
    pub highlights: &'static str,
    pub grammar: LanguageFn,
}

macro_rules! code {
    ($dir:literal, $grammar:expr) => {
        code!(
            $dir,
            $grammar,
            include_str!(concat!("../languages/", $dir, "/highlights.scm"))
        )
    };
    ($dir:literal, $grammar:expr, $highlights:expr) => {
        Code {
            config: include_str!(concat!("../languages/", $dir, "/config.toml")),
            highlights: $highlights,
            grammar: $grammar,
        }
    };
}

/// TypeScript's and TSX's queries are JavaScript's with their own after it
/// (and TSX's JSX).
pub(crate) const CODE: [Code; 16] = [
    code!("python", tree_sitter_python::LANGUAGE),
    code!(
        "typescript",
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        concat!(
            include_str!("../languages/javascript/highlights.scm"),
            include_str!("../languages/typescript/highlights.scm")
        )
    ),
    code!(
        "tsx",
        tree_sitter_typescript::LANGUAGE_TSX,
        concat!(
            include_str!("../languages/javascript/highlights.scm"),
            include_str!("../languages/javascript/highlights-jsx.scm"),
            include_str!("../languages/typescript/highlights.scm")
        )
    ),
    code!("json", tree_sitter_json::LANGUAGE),
    code!("yaml", tree_sitter_yaml::LANGUAGE),
    code!("toml", tree_sitter_toml_ng::LANGUAGE),
    code!("bash", tree_sitter_bash::LANGUAGE),
    code!("c", tree_sitter_c::LANGUAGE),
    code!("cpp", tree_sitter_cpp::LANGUAGE),
    code!("rust", tree_sitter_rust::LANGUAGE),
    code!("go", tree_sitter_go::LANGUAGE),
    code!("java", tree_sitter_java::LANGUAGE),
    code!("r", tree_sitter_r::LANGUAGE),
    code!("lua", tree_sitter_lua::LANGUAGE),
    code!("make", tree_sitter_make::LANGUAGE),
    code!("dockerfile", tree_sitter_containerfile::LANGUAGE),
];

/// Register each grammar, and its language to be built on first use.
pub(crate) fn register(registry: &LanguageRegistry) {
    for code in &CODE {
        let config =
            LanguageConfig::from_toml(code.config).expect("a bundled language config parses");
        let grammar = config
            .grammar
            .clone()
            .expect("a code language names its grammar");
        registry.register_native_grammars([(grammar.clone(), code.grammar)]);
        let highlights = code.highlights;
        registry.register_language(
            config.name.clone(),
            Some(grammar),
            config.matcher.clone(),
            config.hidden,
            None,
            Arc::new(move || {
                let config = config.clone();
                async move {
                    Ok(LoadedLanguage {
                        config,
                        queries: LanguageQueries {
                            highlights: Some(Cow::Borrowed(highlights)),
                            ..LanguageQueries::default()
                        },
                        context_provider: None,
                        toolchain_provider: None,
                        manifest_name: None,
                    })
                }
                .boxed()
            }),
        );
    }
}

/// The language a code file opens in, by its name or extension as the
/// configs list them (`Makefile`, `.zshrc`, `main.py`), in any case: built the
/// first time, else at once. `None` when no code language claims the file.
pub fn load_for_path(
    path: &Path,
    cx: &App,
) -> Option<impl Future<Output = Option<Arc<Language>>> + use<>> {
    let registry = crate::markdown::registry(cx);
    let id = registry.language_for_file_path(path).or_else(|| {
        let lowercase = path.file_name()?.to_str()?.to_lowercase();
        registry.language_for_file_path(Path::new(&lowercase))
    })?;
    let loaded = registry.load_language(id);
    Some(async move { loaded.await.ok()?.ok() })
}

/// Reparse `buffer` each time the registry gains a language while one of
/// its injections (a Markdown fence, an HTML `<script>`) names a language
/// not built yet. Zed leaves such a block pending and starts building its
/// language, but reparses it only on the buffer's next edit; Zed's project
/// does this on registry changes (`lsp_store.rs`), which Lattice does not
/// use. The watch ends once the buffer is gone.
pub fn reparse_on_load(buffer: &Entity<Buffer>, cx: &mut App) {
    let mut changes = crate::markdown::registry(cx).subscribe();
    let buffer = buffer.downgrade();
    cx.spawn(async move |cx| {
        while changes.next().await.is_some() {
            let reparsed = buffer.update(cx, |buffer, cx| {
                if buffer.contains_unknown_injections() {
                    buffer.reparse(cx, false);
                }
            });
            if reparsed.is_err() {
                break;
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
