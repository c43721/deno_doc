// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use deno_ast::ModuleSpecifier;
use deno_doc::html::pages::SymbolPage;
use deno_doc::html::*;
use deno_doc::DocNode;
use deno_doc::DocParser;
use deno_doc::DocParserOptions;
use deno_graph::source::LoadFuture;
use deno_graph::source::LoadOptions;
use deno_graph::source::LoadResponse;
use deno_graph::source::Loader;
use deno_graph::BuildOptions;
use deno_graph::CapturingModuleAnalyzer;
use deno_graph::GraphKind;
use deno_graph::ModuleGraph;
use futures::future;
use indexmap::IndexMap;
use std::fs;
use std::rc::Rc;

struct SourceFileLoader {}

impl Loader for SourceFileLoader {
  fn load(
    &self,
    specifier: &ModuleSpecifier,
    _options: LoadOptions,
  ) -> LoadFuture {
    let result = if specifier.scheme() == "file" {
      let path = specifier.to_file_path().unwrap();
      fs::read(path)
        .map(|content| {
          Some(LoadResponse::Module {
            specifier: specifier.clone(),
            maybe_headers: None,
            content: content.into(),
          })
        })
        .map_err(|err| err.into())
    } else {
      Ok(None)
    };
    Box::pin(future::ready(result))
  }
}

struct EmptyResolver {}

impl HrefResolver for EmptyResolver {
  fn resolve_path(
    &self,
    current: UrlResolveKind,
    target: UrlResolveKind,
  ) -> String {
    href_path_resolve(current, target)
  }

  fn resolve_global_symbol(&self, _symbol: &[String]) -> Option<String> {
    None
  }

  fn resolve_import_href(
    &self,
    _symbol: &[String],
    _src: &str,
  ) -> Option<String> {
    None
  }

  fn resolve_usage(&self, current_resolve: UrlResolveKind) -> Option<String> {
    current_resolve
      .get_file()
      .map(|current_file| current_file.path.to_string())
  }

  fn resolve_source(&self, _location: &deno_doc::Location) -> Option<String> {
    None
  }

  fn resolve_external_jsdoc_module(
    &self,
    _module: &str,
    _symbol: Option<&str>,
  ) -> Option<(String, String)> {
    None
  }
}

async fn get_files(subpath: &str) -> IndexMap<ModuleSpecifier, Vec<DocNode>> {
  let files = fs::read_dir(
    std::env::current_dir()
      .unwrap()
      .join("tests")
      .join("testdata")
      .join(subpath),
  )
  .unwrap();

  let source_files: Vec<ModuleSpecifier> = files
    .into_iter()
    .map(|entry| {
      let entry = entry.unwrap();
      ModuleSpecifier::from_file_path(entry.path()).unwrap()
    })
    .collect();

  let loader = SourceFileLoader {};
  let analyzer = CapturingModuleAnalyzer::default();
  let mut graph = ModuleGraph::new(GraphKind::TypesOnly);
  graph
    .build(
      source_files.clone(),
      &loader,
      BuildOptions {
        module_analyzer: &analyzer,
        ..Default::default()
      },
    )
    .await;

  let parser = DocParser::new(
    &graph,
    &analyzer,
    DocParserOptions {
      diagnostics: false,
      private: false,
    },
  )
  .unwrap();

  let mut source_files = source_files.clone();
  source_files.sort();
  let mut doc_nodes_by_url = IndexMap::with_capacity(source_files.len());
  for source_file in source_files {
    let nodes = parser.parse_with_reexports(&source_file).unwrap();
    doc_nodes_by_url.insert(source_file, nodes);
  }

  doc_nodes_by_url
}

#[tokio::test]
async fn html_doc_files() {
  let files = generate(
    GenerateOptions {
      package_name: None,
      main_entrypoint: None,
      href_resolver: Rc::new(EmptyResolver {}),
      usage_composer: None,
      rewrite_map: None,
      category_docs: None,
      disable_search: false,
      symbol_redirect_map: None,
      default_symbol_map: None,
    },
    get_files("single").await,
  )
  .unwrap();

  let mut file_names = files.keys().collect::<Vec<_>>();
  file_names.sort();

  assert_eq!(
    file_names,
    [
      "./all_symbols.html",
      "./index.html",
      "./~/Bar.html",
      "./~/Bar.prototype.html",
      "./~/Foo.html",
      "./~/Foo.prototype.html",
      "./~/Foobar.html",
      "./~/Foobar.prototype.html",
      "fuse.js",
      "page.css",
      "reset.css",
      "script.js",
      "search.js",
      "search_index.js",
      "styles.css",
    ]
  );

  #[cfg(feature = "tree-sitter")]
  {
    insta::assert_snapshot!(files.get("./all_symbols.html").unwrap());
    insta::assert_snapshot!(files.get("./index.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Bar.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Bar.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foobar.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foobar.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("fuse.js").unwrap());
    insta::assert_snapshot!(files.get("script.js").unwrap());
    insta::assert_snapshot!(files.get("search.js").unwrap());
    insta::assert_snapshot!(files.get("search_index.js").unwrap());
  }
}

#[tokio::test]
async fn html_doc_files_rewrite() {
  let multiple_dir = std::env::current_dir()
    .unwrap()
    .join("tests")
    .join("testdata")
    .join("multiple");
  let mut rewrite_map = IndexMap::new();
  let main_specifier =
    ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap();
  rewrite_map.insert(main_specifier.clone(), ".".to_string());
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("b.ts")).unwrap(),
    "foo".to_string(),
  );

  let files = generate(
    GenerateOptions {
      package_name: None,
      main_entrypoint: Some(main_specifier),
      href_resolver: Rc::new(EmptyResolver {}),
      usage_composer: None,
      rewrite_map: Some(rewrite_map),
      category_docs: None,
      disable_search: false,
      symbol_redirect_map: None,
      default_symbol_map: None,
    },
    get_files("multiple").await,
  )
  .unwrap();

  let mut file_names = files.keys().collect::<Vec<_>>();
  file_names.sort();

  assert_eq!(
    file_names,
    [
      "./all_symbols.html",
      "./index.html",
      "./~/A.html",
      "./~/A.prototype.html",
      "./~/B.html",
      "./~/B.prototype.html",
      "./~/Bar.html",
      "./~/Bar.prototype.html",
      "./~/Baz.foo.html",
      "./~/Baz.html",
      "./~/Foo.bar.html",
      "./~/Foo.html",
      "./~/Foo.prototype.\"><img src=x onerror=alert(1)>.html",
      "./~/Foo.prototype.foo.html",
      "./~/Foo.prototype.html",
      "./~/Foo.prototype.test.html",
      "./~/Foobar.html",
      "./~/Foobar.prototype.html",
      "./~/Hello.html",
      "./~/Hello.world.html",
      "./~/c.html",
      "./~/d.html",
      "./~/qaz.html",
      "foo/index.html",
      "foo/~/default.html",
      "foo/~/x.html",
      "fuse.js",
      "page.css",
      "reset.css",
      "script.js",
      "search.js",
      "search_index.js",
      "styles.css"
    ]
  );

  #[cfg(feature = "tree-sitter")]
  {
    insta::assert_snapshot!(files.get("./all_symbols.html").unwrap());
    insta::assert_snapshot!(files.get("./index.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Bar.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Bar.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Baz.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Baz.foo.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.bar.html").unwrap());
    insta::assert_snapshot!(files
      .get("./~/Foo.prototype.\"><img src=x onerror=alert(1)>.html")
      .unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.prototype.foo.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foo.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foobar.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Foobar.prototype.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Hello.html").unwrap());
    insta::assert_snapshot!(files.get("./~/Hello.world.html").unwrap());
    insta::assert_snapshot!(files.get("foo/index.html").unwrap());
    insta::assert_snapshot!(files.get("foo/~/x.html").unwrap());
    insta::assert_snapshot!(files.get("fuse.js").unwrap());
    insta::assert_snapshot!(files.get("script.js").unwrap());
    insta::assert_snapshot!(files.get("search.js").unwrap());
    insta::assert_snapshot!(files.get("search_index.js").unwrap());
  }
}

#[tokio::test]
async fn symbol_group() {
  let multiple_dir = std::env::current_dir()
    .unwrap()
    .join("tests")
    .join("testdata")
    .join("multiple");

  let doc_nodes_by_url = get_files("multiple").await;

  let mut rewrite_map = IndexMap::new();
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
    ".".to_string(),
  );
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("b.ts")).unwrap(),
    "foo".to_string(),
  );

  let ctx = GenerateCtx::new(
    GenerateOptions {
      package_name: None,
      main_entrypoint: Some(
        ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
      ),
      href_resolver: Rc::new(EmptyResolver {}),
      usage_composer: None,
      rewrite_map: Some(rewrite_map),
      category_docs: None,
      disable_search: false,
      symbol_redirect_map: None,
      default_symbol_map: None,
    },
    None,
    Default::default(),
    doc_nodes_by_url,
  )
  .unwrap();

  let mut files = vec![];

  {
    for (short_path, doc_nodes) in &ctx.doc_nodes {
      let symbol_pages =
        generate_symbol_pages_for_module(&ctx, short_path, doc_nodes);

      files.extend(symbol_pages.into_iter().map(
        |symbol_page| match symbol_page {
          SymbolPage::Symbol {
            breadcrumbs_ctx,
            symbol_group_ctx,
            toc_ctx,
            categories_panel,
          } => {
            let root = ctx.resolve_path(
              UrlResolveKind::Symbol {
                file: short_path,
                symbol: &symbol_group_ctx.name,
              },
              UrlResolveKind::Root,
            );

            let html_head_ctx = pages::HtmlHeadCtx::new(
              &root,
              Some(&symbol_group_ctx.name),
              ctx.package_name.as_ref(),
              Some(short_path),
              false,
            );

            Some(pages::SymbolPageCtx {
              html_head_ctx,
              symbol_group_ctx,
              breadcrumbs_ctx,
              toc_ctx,
              disable_search: false,
              categories_panel,
            })
          }
          SymbolPage::Redirect { .. } => None,
        },
      ));
    }
  }

  #[cfg(feature = "tree-sitter")]
  insta::assert_json_snapshot!(files);
}

#[tokio::test]
async fn symbol_search() {
  let multiple_dir = std::env::current_dir()
    .unwrap()
    .join("tests")
    .join("testdata")
    .join("multiple");

  let doc_nodes_by_url = get_files("multiple").await;

  let mut rewrite_map = IndexMap::new();
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
    ".".to_string(),
  );
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("b.ts")).unwrap(),
    "foo".to_string(),
  );

  let ctx = GenerateCtx::new(
    GenerateOptions {
      package_name: None,
      main_entrypoint: Some(
        ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
      ),
      href_resolver: Rc::new(EmptyResolver {}),
      usage_composer: None,
      rewrite_map: Some(rewrite_map),
      category_docs: None,
      disable_search: false,
      symbol_redirect_map: None,
      default_symbol_map: None,
    },
    None,
    Default::default(),
    doc_nodes_by_url,
  )
  .unwrap();

  let search_index = generate_search_index(&ctx);

  insta::assert_json_snapshot!(search_index);
}

#[tokio::test]
async fn module_doc() {
  let multiple_dir = std::env::current_dir()
    .unwrap()
    .join("tests")
    .join("testdata")
    .join("multiple");

  let doc_nodes_by_url = get_files("multiple").await;

  let mut rewrite_map = IndexMap::new();
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
    ".".to_string(),
  );
  rewrite_map.insert(
    ModuleSpecifier::from_file_path(multiple_dir.join("b.ts")).unwrap(),
    "foo".to_string(),
  );

  let ctx = GenerateCtx::new(
    GenerateOptions {
      package_name: None,
      main_entrypoint: Some(
        ModuleSpecifier::from_file_path(multiple_dir.join("a.ts")).unwrap(),
      ),
      href_resolver: Rc::new(EmptyResolver {}),
      usage_composer: None,
      rewrite_map: Some(rewrite_map),
      category_docs: None,
      disable_search: false,
      symbol_redirect_map: None,
      default_symbol_map: None,
    },
    None,
    FileMode::Single,
    doc_nodes_by_url,
  )
  .unwrap();

  let mut module_docs = vec![];

  for (short_path, doc_nodes) in &ctx.doc_nodes {
    let render_ctx =
      RenderContext::new(&ctx, doc_nodes, UrlResolveKind::File(short_path));
    let module_doc = jsdoc::ModuleDocCtx::new(&render_ctx, short_path);

    module_docs.push(module_doc);
  }

  #[cfg(feature = "tree-sitter")]
  insta::assert_json_snapshot!(module_docs);
}
