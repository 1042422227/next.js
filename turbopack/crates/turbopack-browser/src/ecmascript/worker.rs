use std::io::Write;

use anyhow::Result;
use turbo_rcstr::{RcStr, rcstr};
use turbo_tasks::{ResolvedVc, ValueToString, Vc};
use turbo_tasks_fs::{File, FileContent, FileSystemPath};
use turbopack_core::{
    asset::{Asset, AssetContent},
    chunk::{ChunkingContext, MinifyType},
    code_builder::{Code, CodeBuilder},
    ident::AssetIdent,
    output::{OutputAsset, OutputAssetsReference, OutputAssetsWithReferenced},
    source_map::{GenerateSourceMap, SourceMapAsset},
};
use turbopack_ecmascript::minify::minify;

/// A pre-compiled worker entrypoint that bootstraps workers by reading config from URL params.
///
/// The worker receives a JSON array via URL params where:
/// - Index 0: Array of chunk URLs (-> TURBOPACK_NEXT_CHUNK_URLS, loaded via importScripts)
/// - Index 1+: Values for forwarded globals (in order of `forwarded_globals`)
#[turbo_tasks::value(shared)]
pub struct EcmascriptBrowserWorkerEntrypoint {
    chunking_context: ResolvedVc<Box<dyn ChunkingContext>>,
    /// Global variable names to forward from main thread to worker.
    /// These are assigned to `self` in the worker scope before loading chunks.
    /// Values are passed via URL params at indices 1+.
    forwarded_globals: ResolvedVc<Vec<RcStr>>,
}

#[turbo_tasks::value_impl]
impl EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    pub async fn new(
        chunking_context: ResolvedVc<Box<dyn ChunkingContext>>,
        forwarded_globals: Vc<Vec<RcStr>>,
    ) -> Result<Vc<Self>> {
        Ok(EcmascriptBrowserWorkerEntrypoint {
            chunking_context,
            forwarded_globals: forwarded_globals.to_resolved().await?,
        }
        .cell())
    }

    #[turbo_tasks::function]
    async fn code(self: Vc<Self>) -> Result<Vc<Code>> {
        let this = self.await?;

        let source_maps = *this
            .chunking_context
            .reference_chunk_source_maps(Vc::upcast(self))
            .await?;

        let forwarded_globals = this.forwarded_globals.await?;
        let mut code = generate_worker_bootstrap_code(&*forwarded_globals, source_maps)?;
        if let MinifyType::Minify { mangle } = *this.chunking_context.minify_type().await? {
            code = minify(code, source_maps, mangle)?;
        }

        Ok(code.cell())
    }

    #[turbo_tasks::function]
    async fn ident_for_path(&self) -> Result<Vc<AssetIdent>> {
        let chunk_root_path = self.chunking_context.chunk_root_path().owned().await?;
        let ident = AssetIdent::from_path(chunk_root_path)
            .with_modifier(rcstr!("turbopack worker entrypoint"));
        Ok(ident)
    }

    #[turbo_tasks::function]
    async fn source_map(self: Vc<Self>) -> Result<Vc<SourceMapAsset>> {
        let this = self.await?;
        Ok(SourceMapAsset::new(
            *this.chunking_context,
            self.ident_for_path(),
            Vc::upcast(self),
        ))
    }
}

#[turbo_tasks::value_impl]
impl ValueToString for EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    fn to_string(&self) -> Vc<RcStr> {
        Vc::cell(rcstr!("Ecmascript Browser Worker Entrypoint"))
    }
}

#[turbo_tasks::value_impl]
impl OutputAssetsReference for EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    async fn references(self: Vc<Self>) -> Result<Vc<OutputAssetsWithReferenced>> {
        let mut references = Vec::new();
        references.push(ResolvedVc::upcast(self.to_resolved().await?));
        Ok(OutputAssetsWithReferenced::from_assets(Vc::cell(
            references,
        )))
    }
}

#[turbo_tasks::value_impl]
impl OutputAsset for EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    async fn path(self: Vc<Self>) -> Result<Vc<FileSystemPath>> {
        let this = self.await?;
        let ident = self.ident_for_path();
        Ok(this.chunking_context.chunk_path(
            Some(Vc::upcast(self)),
            ident,
            Some(rcstr!("turbopack-worker")),
            rcstr!(".js"),
        ))
    }
}

#[turbo_tasks::value_impl]
impl Asset for EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    async fn content(self: Vc<Self>) -> Result<Vc<AssetContent>> {
        Ok(AssetContent::file(
            FileContent::Content(File::from(
                self.code()
                    .to_rope_with_magic_comments(|| self.source_map())
                    .await?,
            ))
            .cell(),
        ))
    }
}

#[turbo_tasks::value_impl]
impl GenerateSourceMap for EcmascriptBrowserWorkerEntrypoint {
    #[turbo_tasks::function]
    fn generate_source_map(self: Vc<Self>) -> Vc<FileContent> {
        self.code().generate_source_map()
    }
}

/// Generates the worker bootstrap code as inline JavaScript.
///
/// The worker receives a JSON array via URL params where:
/// - Index 0: Array of chunk URLs (-> TURBOPACK_NEXT_CHUNK_URLS, loaded via importScripts)
/// - Index 1+: Values for forwarded globals (in order of `forwarded_globals`)
///
/// The generated code:
/// 1. Parses the params from URL hash or querystring
/// 2. Sets TURBOPACK_NEXT_CHUNK_URLS and forwarded globals on self via Object.assign
/// 3. Loads chunks via importScripts (in reverse order)
fn generate_worker_bootstrap_code(
    forwarded_globals: &[RcStr],
    _generate_source_map: bool,
) -> Result<Code> {
    let mut code: CodeBuilder = CodeBuilder::default();

    // Generate the Object.assign properties for forwarded globals
    // TURBOPACK_NEXT_CHUNK_URLS is always params[0], then forwarded globals at params[1+]
    let mut global_assignments = vec!["TURBOPACK_NEXT_CHUNK_URLS: params[0]".to_string()];
    for (i, name) in forwarded_globals.iter().enumerate() {
        global_assignments.push(format!("{}: params[{}]", name, i + 1));
    }
    let globals_js = global_assignments.join(",\n    ");

    write!(
        code,
        r##"(function() {{
  var url = new URL(location.href);
  var paramsString = url.searchParams.get("params");
  if (!paramsString && url.hash.startsWith("#params=")) {{
    paramsString = decodeURIComponent(url.hash.slice("#params=".length));
  }}
  if (!paramsString) return;

  var params = JSON.parse(paramsString);

  // Set worker globals before loading chunks
  Object.assign(self, {{
    {0}
  }});

  // Load chunks via importScripts (in reverse order for correct execution)
  var chunkUrls = params[0];
  if (chunkUrls && chunkUrls.length > 0) {{
    importScripts.apply(
      self,
      chunkUrls.map(function(chunk) {{
        return new URL(chunk, location.origin).toString();
      }}).reverse()
    );
  }}
}})();
"##,
        globals_js
    )?;

    Ok(code.build())
}
