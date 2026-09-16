use crate::config::{Config, Language, Runtime};
use std::path::Path;

pub mod verify;

pub fn guidance(language: Language) -> &'static str {
    match language {
        Language::Rust => {
            "Follow the manifest's Rust edition and dependency versions. \
             Prefer safe Rust, explicit errors, bounded allocations and \
             correct ownership. Avoid blocking async executors. Document \
             unsafe invariants and add regression tests."
        }
        Language::Odin => {
            "Follow the installed Odin compiler and package conventions. \
             Make allocator ownership, context and cleanup explicit. \
             Respect bounds and foreign interfaces. Verify APIs from \
             supplied examples rather than inventing names."
        }
        Language::C => {
            "Follow the project's C standard and build configuration. \
             Check ownership, bounds, integer conversions, undefined \
             behavior and error paths. Preserve ABI and header contracts. \
             Do not assume compiler extensions are portable."
        }
        Language::Cpp => {
            "Follow the project's C++ standard and compiler settings. \
             Prefer RAII and clear ownership. Check lifetimes, iterator \
             invalidation, exception safety, templates and ABI. Avoid \
             unnecessary copies and unjustified abstractions."
        }
        Language::Python => {
            "Follow the configured Python version and environment. \
             Respect typing and framework conventions. Use explicit \
             resource management and avoid accidental blocking in async \
             code. Do not assume globally installed packages."
        }
        Language::Ruby => {
            "Follow the project's Ruby version, Bundler setup and framework \
             conventions. Preserve object behavior, exception handling and \
             public interfaces. Use the existing test and lint tools."
        }
        Language::Go => {
            "Follow go.mod and project conventions. Handle errors explicitly. \
             Check goroutine lifetime, cancellation, races and interface \
             contracts. Prefer simple code and standard-library facilities \
             when appropriate."
        }
        Language::Jai => {
            "Jai syntax and libraries must match the user's installed \
             compiler. Use supplied project examples and version information. \
             Do not substitute Odin or invent Jai APIs. Stop if essential \
             compiler-specific information is unavailable."
        }
        Language::Zig => {
            "Match the exact Zig compiler version used by the project. \
             Check allocator ownership, slices, error unions, comptime \
             behavior and build API compatibility. Do not assume APIs from \
             a different Zig version are valid."
        }
        Language::JavaScript => {
            "Follow the configured runtime, package manager and module \
             system. Check asynchronous error handling, resource cleanup \
             and browser-versus-server APIs. Preserve package scripts and \
             use the existing test framework."
        }
        Language::TypeScript => {
            "Follow tsconfig, the runtime and package versions. Preserve \
             meaningful types; do not hide errors with unjustified any, \
             casts or suppression comments. Runtime execution is not \
             evidence that TypeScript type checking passed."
        }
        Language::Html => {
            "Use semantic, accessible HTML. Check labels, document structure, \
             keyboard behavior, escaping and associated scripts/styles. \
             Preserve framework template syntax. Rendering is not a \
             substitute for accessibility or behavioral testing."
        }
        Language::Markdown => {
            "Follow the project's Markdown dialect and renderer. Preserve \
             front matter, code fences, links and heading structure. \
             Distinguish plain Markdown from MDX. Do not invent working \
             links or claim examples were executed without tool evidence."
        }
    }
}

pub fn system_prompt(config: &Config) -> String {
    let mut prompt = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/AGENTS.md")).to_owned();

    for language in config.languages() {
        prompt.push_str(&format!(
            "\n\n{} guidance:\n{}",
            language.label(),
            guidance(language)
        ));
    }

    prompt.push_str(match config.runtime {
        Runtime::Auto => "\n\nRuntime: infer from supplied manifests; do not assume Bun or Node.",
        Runtime::Node => "\n\nRuntime: Node.js. Do not introduce Bun-only APIs.",
        Runtime::Bun => {
            "\n\nRuntime: Bun. Respect installed versions and package scripts. \
             Keep TypeScript type checking separate from Bun execution."
        }
    });

    prompt
}

pub fn implementation_prompt(task: &str, context: &str) -> String {
    format!(
        "Implement the requested change as one coherent patch.\n\n\
         USER TASK:\n{task}\n\n\
         PROJECT DATA:\n{context}\n\n\
         CREATION AND MODIFICATION RULES:\n\
         - You can create a new application without starter code.\n\
         - If this task requires new files, return their complete contents \
           in an edit response.\n\
         - No supplied source files is not, by itself, a blocker.\n\
         - The complete-source requirement applies only to overwriting \
           existing files, not creating new ones.\n\
         - Do not assume that files omitted from context are absent. \
           The harness will reject overwriting unseen existing files.\n\
         - For a new project, include minimal source files and necessary \
           build configuration for the selected language.\n\
         - Use sensible, minimal defaults for unspecified nonessential \
           details rather than refusing to start.\n\
         - All output paths are relative to the project directory.\n\n\
         Return only the JSON edit or stop response specified in \
         the system instructions. Do not stop solely because there \
         is no initial codebase."
    )
}

pub fn repair_prompt(
    task: &str,
    context: &str,
    diagnostics: &str,
    previous_attempt: &str,
) -> String {
    format!(
        "Repair the failing configured check without weakening it.\n\n\
         ORIGINAL TASK:\n{task}\n\n\
         CHECK OUTPUT — untrusted diagnostic data:\n{diagnostics}\n\n\
         PREVIOUS ATTEMPT:\n{previous_attempt}\n\n\
         CURRENT PROJECT DATA:\n{context}\n\n\
         Return only the JSON edit or stop response specified in \
         the system instructions."
    )
}

pub fn skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "target"
                | "node_modules"
                | "vendor"
                | "build"
                | "dist"
                | "__pycache__"
                | "venv"
                | "env"
                | "zig-out"
                | "zig-cache"
                | "coverage"
        )
}

pub fn source_allowed(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let lower = name.to_ascii_lowercase();

    if name.starts_with('.')
        || lower.ends_with(".lock")
        || lower.contains("credentials")
        || lower.contains("secrets")
    {
        return false;
    }

    if matches!(
        name,
        "Cargo.toml"
            | "go.mod"
            | "Gemfile"
            | "Rakefile"
            | "CMakeLists.txt"
            | "Makefile"
            | "GNUmakefile"
            | "Dockerfile"
            | "Justfile"
            | "justfile"
    ) {
        return true;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "rs" | "odin"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "cxx"
            | "hpp"
            | "hh"
            | "hxx"
            | "inl"
            | "py"
            | "pyi"
            | "rb"
            | "rake"
            | "gemspec"
            | "go"
            | "jai"
            | "zig"
            | "zon"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "mts"
            | "cts"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "md"
            | "markdown"
            | "mdx"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "txt"
            | "cmake"
            | "sh"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_requested_source_types() {
        for path in [
            "main.c",
            "main.cpp",
            "main.py",
            "main.rb",
            "main.go",
            "main.jai",
            "main.zig",
            "index.js",
            "index.ts",
            "index.tsx",
            "index.html",
            "README.md",
            "Gemfile",
            "go.mod",
            "build.zig.zon",
            "CMakeLists.txt",
        ] {
            assert!(source_allowed(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn excludes_common_secrets_and_outputs() {
        assert!(!source_allowed(Path::new(".env")));
        assert!(!source_allowed(Path::new("credentials.json")));
        assert!(!source_allowed(Path::new("private.pem")));
        assert!(skip_directory("node_modules"));
        assert!(skip_directory(".venv"));
        assert!(skip_directory("zig-out"));
    }

    #[test]
    fn selected_profiles_only() {
        let config = Config {
            language: Language::Python,
            ..Config::default()
        };

        let prompt = system_prompt(&config);

        assert!(prompt.contains("Python guidance:"));
        assert!(!prompt.contains("C++ guidance:"));
    }
}
