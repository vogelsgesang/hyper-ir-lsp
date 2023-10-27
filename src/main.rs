use std::collections::HashMap;

use dashmap::DashMap;
use hir_language_server::hir_index::{create_index, HIRIndex, UseDefKind, UseDefList};
use hir_language_server::hir_parser::{parse_from_str, ParserResult};
use hir_language_server::hir_tokenizer::Span;
use hir_language_server::semantic_token::{
    convert_to_lsp_tokens, semantic_tokens_from_tokens, HIRSemanticToken, LEGEND_TYPE,
};
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::request::GotoDeclarationResponse;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, Rope>,
    semantic_token_map: DashMap<String, Vec<HIRSemanticToken>>,
    index_map: DashMap<String, HIRIndex>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Hyper IR Language Server".to_string(),
                version: Some("1".to_string()),
            }),
            offset_encoding: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: {
                                TextDocumentRegistrationOptions {
                                    document_selector: Some(vec![DocumentFilter {
                                        language: Some("hir".to_string()),
                                        scheme: None,
                                        pattern: None,
                                    }]),
                                }
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: WorkDoneProgressOptions::default(),
                                legend: SemanticTokensLegend {
                                    token_types: LEGEND_TYPE.into(),
                                    token_modifiers: vec![],
                                },
                                range: None,
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                declaration_provider: Some(DeclarationCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let definition = || -> Option<GotoDefinitionResponse> {
            let uri = &params.text_document_position_params.text_document.uri;
            let (origin_selection_range, ranges) =
                self.get_use_def_ranges(&params.text_document_position_params, UseDefKind::Def)?;
            let links = ranges
                .iter()
                .map(|&range| LocationLink {
                    origin_selection_range: Some(origin_selection_range),
                    target_uri: uri.clone(),
                    target_range: range,
                    target_selection_range: range,
                })
                .collect::<Vec<_>>();

            Some(GotoDefinitionResponse::Link(links))
        }();
        Ok(definition)
    }

    async fn goto_declaration(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let decl = || -> Option<GotoDeclarationResponse> {
            let (origin_selection_range, ranges) =
                self.get_use_def_ranges(&params.text_document_position_params, UseDefKind::Decl)?;
            let uri = &params.text_document_position_params.text_document.uri;
            let links = ranges
                .iter()
                .map(|&range| LocationLink {
                    origin_selection_range: Some(origin_selection_range),
                    target_uri: uri.clone(),
                    target_range: range,
                    target_selection_range: range,
                })
                .collect::<Vec<_>>();

            Some(GotoDeclarationResponse::Link(links))
        }();
        Ok(decl)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let reference_list = || -> Option<Vec<Location>> {
            let (_origin_selection_range, ranges) =
                self.get_use_def_ranges(&params.text_document_position, UseDefKind::Use)?;
            let ret = ranges
                .iter()
                .map(|&range| {
                    Location::new(
                        params.text_document_position.text_document.uri.clone(),
                        range,
                    )
                })
                .collect::<Vec<_>>();
            Some(ret)
        }();
        Ok(reference_list)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        self.client
            .log_message(MessageType::LOG, "semantic_token_full")
            .await;
        let lsp_tokens = || -> Option<Vec<SemanticToken>> {
            let sem_tokens = self.semantic_token_map.get(&uri)?;
            let rope = self.document_map.get(&uri)?;
            let lsp_tokens = convert_to_lsp_tokens(&rope, &sem_tokens);
            Some(lsp_tokens)
        }();
        if let Some(semantic_token) = lsp_tokens {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })));
        }
        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        self.client
            .log_message(MessageType::LOG, "document_symbol")
            .await;

        let symbols = || -> Option<DocumentSymbolResponse> {
            let index = self.index_map.get(&uri)?;
            let rope = self.document_map.get(&uri)?;
            let mut symbols = Vec::<DocumentSymbol>::new();

            fn get_def_symbols<'a>(
                ud: &'a HashMap<String, UseDefList>,
                rope: &'a Rope,
                kind: SymbolKind,
            ) -> impl Iterator<Item = tower_lsp::lsp_types::DocumentSymbol> + 'a {
                ud.iter().flat_map(move |f| {
                    f.1.defs.iter().filter_map(move |def| {
                        Some(DocumentSymbol {
                            name: f.0.to_string(),
                            detail: None,
                            kind,
                            tags: None,
                            range: range_to_lsp(rope, def)?,
                            selection_range: range_to_lsp(rope, def)?,
                            deprecated: None,
                            children: None,
                        })
                    })
                })
            }

            symbols.extend(get_def_symbols(
                &index.global_vars,
                &rope,
                SymbolKind::CONSTANT,
            ));

            symbols.extend(index.function_bodies.iter().filter_map(|f| {
                Some(DocumentSymbol {
                    name: f.name.0.clone(),
                    detail: None,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    range: range_to_lsp(&rope, &f.complete_range)?,
                    selection_range: range_to_lsp(&rope, &f.name.1)?,
                    deprecated: None,
                    children: None,
                    // We could add the labels as children of the functions.
                    // I decided against it, because it looks a bit too crowded.
                    // children: Some(get_def_symbols(&f.labels, &rope, SymbolKind::KEY).collect::<Vec<_>>()),
                })
            }));

            Some(DocumentSymbolResponse::from(symbols))
        }();

        self.client
            .log_message(MessageType::LOG, &format!("symbols {:?}", symbols))
            .await;

        Ok(symbols)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let folding_ranges = || -> Option<Vec<FoldingRange>> {
            let uri = &params.text_document.uri;
            let uri_str = uri.to_string();
            let index = self.index_map.get(&uri_str)?;
            let rope = self.document_map.get(&uri_str)?;
            let mut folding_ranges = Vec::<FoldingRange>::new();

            folding_ranges.extend(index.function_bodies.iter().filter_map(|f| {
                let collapsed_text = format!("{} basic blocks", f.labels.len());
                let range = range_to_lsp(&rope, &f.complete_range)?;
                Some(FoldingRange {
                    start_line: range.start.line,
                    start_character: None,
                    end_line: range.end.line,
                    end_character: None,
                    kind: None,
                    collapsed_text: Some(collapsed_text),
                })
            }));

            folding_ranges.extend(
                index
                    .function_bodies
                    .iter()
                    .flat_map(|e| e.basic_blocks.iter())
                    .filter_map(|bb| {
                        let collapsed_text = format!("{} instructions", bb.instructions.len());
                        let range = range_to_lsp(&rope, &bb.span)?;
                        Some(FoldingRange {
                            start_line: range.start.line,
                            start_character: None,
                            end_line: range.end.line,
                            end_character: None,
                            kind: None,
                            collapsed_text: Some(collapsed_text),
                        })
                    }),
            );

            Some(folding_ranges)
        }();
        return Ok(folding_ranges);
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let inlay_hints = || -> Option<Vec<InlayHint>> {
            let uri = &params.text_document.uri;
            let uri_str = uri.to_string();
            let index = self.index_map.get(&uri_str)?;
            let rope = self.document_map.get(&uri_str)?;
            let mut inlay_hints = Vec::<InlayHint>::new();

            inlay_hints.extend(index.function_bodies.iter().filter_map(|f| {
                Some(InlayHint {
                    position: offset_to_lsp_pos(&rope, f.complete_range.end)?,
                    label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                        value: f.name.0.clone(),
                        tooltip: None,
                        location: Some(Location {
                            uri: uri.clone(),
                            range: range_to_lsp(&rope, &f.name.1)?,
                        }),
                        command: None,
                    }]),
                    kind: None,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: Some(true),
                    data: None,
                })
            }));

            Some(inlay_hints)
        }();
        return Ok(inlay_hints);
    }
}

struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let rope = ropey::Rope::from_str(&params.text);
        self.document_map
            .insert(params.uri.to_string(), rope.clone());

        let ParserResult {
            tokens,
            stmts,
            errors,
        } = parse_from_str(&rope.to_string());

        self.semantic_token_map
            .insert(params.uri.to_string(), semantic_tokens_from_tokens(&tokens));

        self.index_map
            .insert(params.uri.to_string(), create_index(&tokens, &stmts));

        let diagnostics = errors
            .into_iter()
            .filter_map(|item| {
                let (message, span) = match item.reason() {
                    chumsky::error::SimpleReason::Unclosed { span, delimiter } => {
                        (format!("Unclosed delimiter {}", delimiter), span.clone())
                    }
                    chumsky::error::SimpleReason::Unexpected => (
                        format!(
                            "{}, expected {}",
                            if item.found().is_some() {
                                "Unexpected token in input"
                            } else {
                                "Unexpected end of input"
                            },
                            if item.expected().len() == 0 {
                                "something else".to_string()
                            } else {
                                item.expected()
                                    .map(|expected| match expected {
                                        Some(expected) => expected.to_string(),
                                        None => "end of input".to_string(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        ),
                        item.span(),
                    ),
                    chumsky::error::SimpleReason::Custom(msg) => (msg.to_string(), item.span()),
                };

                || -> Option<Diagnostic> {
                    let start_position = offset_to_lsp_pos(&rope, span.start)?;
                    let end_position = offset_to_lsp_pos(&rope, span.end)?;
                    Some(Diagnostic::new_simple(
                        Range::new(start_position, end_position),
                        message,
                    ))
                }()
            })
            .collect::<Vec<_>>();

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }

    fn get_use_def_ranges(
        &self,
        pos: &TextDocumentPositionParams,
        ud: UseDefKind,
    ) -> Option<(Range, Vec<Range>)> {
        let uri_str = pos.text_document.uri.to_string();
        let rope = self.document_map.get(&uri_str)?;
        let offset = lsp_pos_to_offset(&rope, &pos.position)?;

        let index = self.index_map.get(&uri_str)?;
        let symbol = index.find_symbol_at_position(offset)?;
        let spans = index
            .get_by_symbol_kind(
                symbol.symbol_kind,
                symbol.func_body_id.map(|id| &index.function_bodies[id]),
            )
            .get(&symbol.name)?
            .get_use_def_kind(ud);

        let origin_selection_range = range_to_lsp(&rope, &symbol.span)?;
        let ranges = spans
            .iter()
            .filter_map(|span| range_to_lsp(&rope, span))
            .collect::<Vec<_>>();
        Some((origin_selection_range, ranges))
    }
}

fn lsp_pos_to_offset(rope: &Rope, pos: &Position) -> Option<usize> {
    let char = rope.try_line_to_char(pos.line as usize).ok()?;
    Some(char + pos.character as usize)
}

fn offset_to_lsp_pos(rope: &Rope, pos: usize) -> Option<Position> {
    let line = rope.try_byte_to_line(pos).ok()?;
    let first = rope.try_line_to_char(line).ok()?;
    let character = rope.try_byte_to_char(pos).ok()? - first;
    Some(Position {
        line: line.try_into().ok()?,
        character: character.try_into().ok()?,
    })
}

fn range_to_lsp(rope: &Rope, span: &Span) -> Option<Range> {
    Some(Range {
        start: offset_to_lsp_pos(rope, span.start).unwrap(),
        end: offset_to_lsp_pos(rope, span.end).unwrap(),
    })
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        document_map: DashMap::new(),
        semantic_token_map: DashMap::new(),
        index_map: DashMap::new(),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
