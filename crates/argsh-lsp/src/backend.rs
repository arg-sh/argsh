use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use argsh_syntax::document::{analyze, DocumentAnalysis};

use crate::codelens;
use crate::completion;
use crate::diagnostics;
use crate::goto_def;
use crate::hover;
use crate::preview;
use crate::rename;
use crate::resolver::{self, ResolvedImports};
use crate::symbols;

/// Client-provided settings from initializationOptions.
#[derive(Debug, Clone)]
struct Settings {
    resolve_depth: usize,
    code_lens_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            resolve_depth: resolver::DEFAULT_MAX_DEPTH,
            code_lens_enabled: true,
        }
    }
}

pub struct Backend {
    client: Client,
    documents: DashMap<Url, DocumentState>,
    settings: std::sync::RwLock<Settings>,
}

pub struct DocumentState {
    pub content: String,
    pub analysis: DocumentAnalysis,
    pub is_argsh: bool,
    pub imports: ResolvedImports,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            settings: std::sync::RwLock::new(Settings::default()),
        }
    }

    fn update_document(&self, uri: &Url, content: String) {
        let analysis = analyze(&content);
        let is_argsh = analysis.has_source_argsh
            || analysis.has_argsh_shebang
            || analysis
                .functions
                .iter()
                .any(|f| f.calls_args || f.calls_usage);

        // Resolve cross-file imports
        let imports = if is_argsh {
            if let Ok(path) = uri.to_file_path() {
                let depth = self.settings.read().unwrap().resolve_depth;
                resolver::resolve_imports(&analysis, &path, depth)
            } else {
                ResolvedImports::default()
            }
        } else {
            ResolvedImports::default()
        };

        self.documents.insert(
            uri.clone(),
            DocumentState {
                content,
                analysis,
                is_argsh,
                imports,
            },
        );
    }

    async fn publish_diagnostics(&self, uri: &Url) {
        if let Some(doc) = self.documents.get(uri) {
            if !doc.is_argsh {
                // Not an argsh file — clear any stale diagnostics.
                self.client
                    .publish_diagnostics(uri.clone(), vec![], None)
                    .await;
                return;
            }
            let diags = diagnostics::generate_diagnostics(&doc.analysis, &doc.imports, &doc.content);
            self.client
                .publish_diagnostics(uri.clone(), diags, None)
                .await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Read client settings from initializationOptions
        if let Some(opts) = params.initialization_options {
            let mut settings = self.settings.write().unwrap();
            if let Some(depth) = opts.get("resolveDepth").and_then(|v| v.as_u64()) {
                settings.resolve_depth = depth as usize;
            }
            if let Some(enabled) = opts.get("codeLensEnabled").and_then(|v| v.as_bool()) {
                settings.code_lens_enabled = enabled;
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "'".to_string(),
                        "@".to_string(),
                        ":".to_string(),
                        "~".to_string(),
                        "-".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["argsh.preview".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "argsh-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "argsh-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        self.update_document(&uri, content);
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(&uri, change.text);
            self.publish_diagnostics(&uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let trigger = params
            .context
            .as_ref()
            .and_then(|ctx| ctx.trigger_character.as_deref());

        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            let items = completion::completions(&doc.analysis, position, trigger, &doc.content);
            if items.is_empty() {
                return Ok(None);
            }
            return Ok(Some(CompletionResponse::Array(items)));
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            if let Some(location) =
                goto_def::goto_definition(&doc.analysis, &doc.imports, position, &doc.content, &uri)
            {
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }
        }
        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            let syms = symbols::document_symbols(&doc.analysis);
            return Ok(Some(DocumentSymbolResponse::Nested(syms)));
        }
        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            return Ok(hover::hover(&doc.analysis, &doc.imports, position, &doc.content));
        }
        Ok(None)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        if !self.settings.read().unwrap().code_lens_enabled {
            return Ok(None);
        }
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            let lenses = codelens::code_lenses(&doc.analysis, &uri);
            return Ok(Some(lenses));
        }
        Ok(None)
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "argsh.preview" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Url>(uri_val.clone()) {
                        if let Some(doc) = self.documents.get(&uri) {
                            let html = preview::generate_preview(&doc.analysis, &doc.content);
                            return Ok(Some(serde_json::Value::String(html)));
                        }
                    }
                }
                Ok(None)
            }
            "argsh.exportMcpJson" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Url>(uri_val.clone()) {
                        if let Some(doc) = self.documents.get(&uri) {
                            let json = preview::export_mcp_json(&doc.analysis);
                            return Ok(Some(serde_json::Value::String(json)));
                        }
                    }
                }
                Ok(None)
            }
            "argsh.exportYaml" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Url>(uri_val.clone()) {
                        if let Some(doc) = self.documents.get(&uri) {
                            let yaml = preview::export_yaml(&doc.analysis, &doc.content);
                            return Ok(Some(serde_json::Value::String(yaml)));
                        }
                    }
                }
                Ok(None)
            }
            "argsh.exportJson" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Url>(uri_val.clone()) {
                        if let Some(doc) = self.documents.get(&uri) {
                            let json = preview::export_docgen_json(&doc.analysis);
                            return Ok(Some(serde_json::Value::String(json)));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            let edits = crate::format::format_document(&doc.content);
            if edits.is_empty() {
                return Ok(None);
            }
            return Ok(Some(edits));
        }
        Ok(None)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            return Ok(rename::prepare_rename(&doc.analysis, position, &doc.content));
        }
        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        if let Some(doc) = self.documents.get(&uri) {
            if !doc.is_argsh {
                return Ok(None);
            }
            return Ok(rename::rename(
                &doc.analysis,
                position,
                &new_name,
                &doc.content,
                &uri,
            ));
        }
        Ok(None)
    }
}
