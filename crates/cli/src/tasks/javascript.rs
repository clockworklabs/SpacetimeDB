use regex::Regex;
use rolldown::{Bundler, BundlerOptions, Either, SourceMapType};
use rolldown_utils::indexmap::FxIndexMap;
use rolldown_utils::js_regex::HybridRegex;
use rolldown_utils::pattern_filter::StringOrRegex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime")
    })
}

/// Synchronously run an async future from a non-async function.
///
/// - If we're already inside a Tokio runtime, we switch to the blocking pool
///   and `block_on` the future (prevents scheduler starvation).
/// - Otherwise, we use a shared, long-lived runtime.
pub fn run_blocking<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    match Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => runtime().block_on(fut),
    }
}

pub(crate) fn build_javascript(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    let cwd = fs::canonicalize(project_path)?;
    let mut bundler = Bundler::new(BundlerOptions {
        input: Some(vec!["./src/index.ts".to_string().into()]),
        cwd: Some(cwd.clone()),
        sourcemap: Some(SourceMapType::Inline),
        external: Some(rolldown::IsExternal::StringOrRegex(vec![
            // Mark the bindings as external so we don't get a warning.
            StringOrRegex::Regex(HybridRegex::Optimize(Regex::new("spacetime:sys.*").unwrap())),
        ])),
        platform: Some(rolldown::Platform::Browser), // Browser is the correct choice here because it doesn't inject Node.js polyfills.
        shim_missing_exports: Some(false),
        name: None,
        entry_filenames: None,
        chunk_filenames: None, // The pattern to use for naming shared chunks created when code-splitting
        css_entry_filenames: None,
        css_chunk_filenames: None, // We're not doing CSS
        asset_filenames: None,     // assets/[name]-[hash][extname]
        sanitize_filename: Some(rolldown::SanitizeFilename::Boolean(true)), // Replace characters that are invalid in filenames with underscores
        dir: None, // The output directory to write to. We only want a single output file, so we won't set this.
        file: Some("./dist/bundle.js".into()), // The output file to write to. We want a single output file.
        format: Some(rolldown::OutputFormat::Esm), // We want to use ES Modules in SpacetimeDB
        exports: Some(rolldown::OutputExports::None), // Let Rolldown decide based on what the module exports (we could probably also use Named here)
        globals: None, // We don't have any external dependencies except for `spacetimedb` which is a dependency and declares all its globals
        generated_code: Some(rolldown::GeneratedCodeOptions::es2015()),
        es_module: Some(rolldown::EsModuleFlag::IfDefaultProp), // See https://rollupjs.org/configuration-options/#output-esmodule
        drop_labels: None,
        hash_characters: None,           // File name hash characters, we don't care
        banner: None,                    // String to prepend to the bundle
        footer: None,                    // String to append to the bundle
        intro: None,                     // Similar to the above, but inside the wrappers
        outro: None,                     // Similar to the above, but inside the wrappers
        sourcemap_base_url: None,        // Absolute URLs for the source map
        sourcemap_ignore_list: None,     // See https://rollupjs.org/configuration-options/#output-sourcemapignorelist
        sourcemap_path_transform: None,  // Function to transform source map paths
        sourcemap_debug_ids: Some(true), // Seems like a good idea. See: https://rollupjs.org/configuration-options/#output-sourcemapdebugids
        module_types: None, // Lets you associate file extensions with module types, e.g. `.data` -> `json`. We don't need this.
        // Wrapper around https://docs.rs/oxc_resolver/latest/oxc_resolver/struct.ResolveOptions.html, see also https://rolldown.rs/guide/features#module-resolution
        resolve: Some(rolldown::ResolveOptions {
            // Prefer environment-neutral exports
            condition_names: Some(vec!["production".into(), "import".into(), "default".into()]),
            main_fields: Some(vec!["exports".into(), "module".into(), "main".into()]),
            extensions: Some(vec![
                ".ts".into(),
                ".tsx".into(),
                ".mjs".into(),
                ".js".into(),
                ".cjs".into(),
                ".json".into(),
            ]),
            symlinks: Some(true),
            ..Default::default()
        }),
        treeshake: rolldown::TreeshakeOptions::Option(rolldown::InnerOptions {
            module_side_effects: rolldown::ModuleSideEffects::Boolean(true), // TODO: SpacetimeDB currently relies on `import './runtime'` to set up the environment, so we can't tree-shake that away.
            annotations: Some(true), // Respect the `/* @__PURE__ */` annotations that tools like Terser use to identify pure functions for tree-shaking.
            manual_pure_functions: None, // Don't manually specify any pure functions.
            unknown_global_side_effects: Some(true), // Default, basically if there is an unknown global, assume it has side effects.
            commonjs: Some(true), // Enable some optimizations for CommonJS modules, even though we don't use any. This is the default.
            property_read_side_effects: Some(rolldown::PropertyReadSideEffects::Always), // Assume that property reads can have side effects. This is safest for users who might use getters with side effects.
            property_write_side_effects: Some(rolldown::PropertyWriteSideEffects::Always), // Assume that property writes can have side effects. This is safest for users who might use setters with side effects.
        }),
        experimental: None, // None for now, although be aware that Rollup has an experimental `perf` option.
        minify: Some(rolldown::RawMinifyOptions::Bool(false)), // Disable minification until we have proper support for source maps.
        define: Some(FxIndexMap::from_iter([
            // TODO(cloutiertyler): I actually think we should probably just always do production mode event in debug builds
            (
                "process.env.NODE_ENV".to_string(),
                if build_debug { "development" } else { "production" }.into(),
            ),
        ])),
        extend: Some(false), // Not relevant for us, this is for extending global variables in UMD/IIFE bundles and we have ESM only.
        profiler_names: None, // Unclear what this is, choosing the default.
        keep_names: None,    // Unclear what this is, choosing the default.
        inject: None,        // Unclear on why we'd need this, choosing the default.
        external_live_bindings: Some(true), // Don't assume that external bindings are going to change over time. Generates more optimized code.
        inline_dynamic_imports: Some(false), // Don't muck with dynamic imports, we want to keep them as-is.
        advanced_chunks: None,              // Not relevant to us, this is for advanced code-splitting strategies.
        checks: Some(rolldown::ChecksOptions {
            circular_dependency: Some(true),           // Check circular dependencies
            eval: Some(false),                         // We don't care about eval
            missing_global_name: Some(true), // Warn if a global variable is missing a name in the output bundle
            missing_name_option_for_iife_export: None, // Don't care, we don't use IIFE
            mixed_export: Some(false),       // Don't care about mixed exports
            unresolved_entry: Some(true),    // If the entry point is unresolved, that's a problem
            unresolved_import: Some(true),   // If an import is unresolved, that's a problem
            filename_conflict: Some(true),
            common_js_variable_in_esm: Some(true),
            import_is_undefined: Some(true),
            empty_import_meta: Some(true),
            configuration_field_conflict: Some(true),
            prefer_builtin_feature: Some(true),
        }),
        transform: Some(rolldown::BundlerTransformOptions {
            jsx: None,                                       // Don't transform JSX
            target: Some(Either::Left("esnext".to_owned())), // Default, no transformation
            assumptions: None, // No compiler assumptions, we don't need to minmax output size
            decorator: None,   // Disable experimental decorators
            typescript: Some(rolldown::TypeScriptOptions {
                jsx_pragma: None,                                     // I am unclear on what this is
                jsx_pragma_frag: None,                                // I am unclear on what this is
                only_remove_type_imports: Some(true),                 // I am assuming we just want to strip these
                allow_namespaces: Some(false),                        // No namespaces, only allow JS + types
                allow_declare_fields: Some(true), // Allow `declare` fields in classes and strip these
                remove_class_fields_without_initializer: Some(false), // Leave them there to not mess with behavior
                declaration: None, // We don't need to generate any declaration files, these are not libraries, although you could imagine this in the future
                rewrite_import_extensions: Some(Either::Left(true)), // Rewrite .ts/.tsx extensions to .js/.jsx, not really relevant for us since we only have a single entry point
            }),
            plugins: None,
        }),
        watch: None,                                         // We don't need watch mode
        legal_comments: Some(rolldown::LegalComments::None), // We don't need any legal comments
        polyfill_require: Some(false),                       // We don't need to polyfill require, only ESM here
        defer_sync_scan_data: None,                          // Unclear what this is
        make_absolute_externals_relative: None, // See https://rollupjs.org/configuration-options/#makeabsoluteexternalsrelative
        debug: None,                            // This is undocumented
        invalidate_js_side_cache: None,
        log_level: Some(rolldown::LogLevel::Debug), // Default logging
        on_log: None,                               // Don't need it
        preserve_modules: Some(false),              // We want a single output file
        virtual_dirname: None,                      // Requires preserve_modules to be true
        preserve_modules_root: None,                // Only relevant if preserve_modules is true
        preserve_entry_signatures: None, // Default is fine, see https://rollupjs.org/configuration-options/#preserveentrysignatures
        optimization: None,              // Defaults are fine
        top_level_var: Some(false),      // This is the safer choice since we'll keep vars scoped to modules
        minify_internal_exports: Some(true), // Sure
        context: None,                   // We don't want a top level `this` in modules
        tsconfig: Some(cwd.join("tsconfig.json").to_string_lossy().into_owned()),
    })?;

    let bundle_output = run_blocking(async move { bundler.write().await })?;

    bundle_output.warnings.into_iter().for_each(|w| {
        eprintln!("Rolldown warning: {w}");
    });

    Ok(project_path.join("dist").join("bundle.js"))
}
