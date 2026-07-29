export interface Paper { id:string; title:string; authors_json:string; year:number|null; doi:string|null; arxiv_id:string|null; canonical_sha256:string|null; source_url:string|null; note_path:string|null; deleted_at:string|null; created_at:string; updated_at:string }
export interface Project { id:string; slug:string; name:string; purpose:string; parent_id:string|null; created_at:string; updated_at:string }
export interface Task { id:string; kind:string; state:string; input_json:string; paper_id:string|null; project_id:string|null; thread_id:string|null; error:string|null; created_at:string; updated_at:string }
export interface Dashboard { papers:Paper[]; projects:Project[]; tasks:Task[]; inbox:Paper[]; trash_count:number; project_memberships:Record<string,string[]> }
export interface Evidence { paper_id:string; revision:string; page:number; section:string|null; locator:string|null; kind:string }
export interface PaperAnalysis { takeaway?:string; research_question?:string; contribution?:string; method?:string; experimental_design?:string; baselines?:string[]; results?:string[]; limitations?:string[]; assumptions?:string[]; reproducibility?:string; evidence?:Evidence[]; [key:string]:unknown }
export interface PaperDetail { paper:Paper; analysis:PaperAnalysis|null; projects:string[]; relations:Array<{source:string;target:string;type:string;hypothesis:boolean}> }
export interface ProjectImpact { direct_papers:number; descendant_projects:number; descendant_papers:number }
export interface PaperImpact { project_references:number; graph_edges:number; revisions:number }
export type KnowledgeKind="paper"|"concept"|"method"|"dataset"|"finding"
export interface GraphNode { id:string; kind:KnowledgeKind; label:string; description:string; paper_id:string|null }
export interface GraphEdge { id:string; source:string; target:string; relation_type:string; hypothesis:boolean; confidence:number; evidence:Evidence[] }
export interface GraphPayload { nodes:GraphNode[]; edges:GraphEdge[] }
export interface SearchResult { entity_type:string; entity_id:string; title:string; snippet:string }
export interface StreamEvent { id:number; type:string; task_id:string; payload:Record<string,unknown>; created_at:string }
export interface Activity { id:number; taskId:string; type:string; label:string; createdAt:string }
export interface CodexRunSettings { model:string; reasoning_effort:string; service_tier:string|null }
export interface CodexModel { id:string; display_name:string; default_reasoning_effort:string; supported_reasoning_efforts:string[]; supports_fast:boolean }
export interface CodexCapabilities { default:CodexRunSettings; models:CodexModel[]; supports_dynamic_tools:boolean }
export interface Conversation { id:string; title:string; thread_id:string|null; status:string; model:string|null; reasoning_effort:string|null; service_tier:string|null; archived_at:string|null; created_at:string; updated_at:string }
export interface ConversationScope { conversation_id?:string; scope_type:"paper"|"project"|"global"; scope_id:string|null; added_at?:string }
export interface MessageCitation { id:string; message_id:string; paper_id:string; revision:string; page:number; section:string|null; locator:string|null; quote:string; prefix:string; suffix:string; explanation:string; match_status:string }
export type ResearchMode="auto"|"explicit"
export type EvidenceLevel="metadata"|"abstract"|"fulltext"
export type CandidateStatus="candidate"|"importing"|"imported"|"dismissed"
export type SearchRunState="running"|"completed"|"partial"|"failed"|"cancelled"
export type ResearchTrigger="automatic"|"explicit"
export type ResearchProgressPhase="research-planning"|"research-searching"|"research-deduplicating"|"research-inspecting-abstract"|"research-fetching-fulltext"|"research-saving-candidates"|"research-partial"
export interface DiscoveredWork { id:string; canonical_key:string; doi:string|null; arxiv_id:string|null; openalex_id:string|null; title:string; authors:string[]; year:number|null; abstract_text:string|null; source_url:string; pdf_url:string|null; evidence_level:EvidenceLevel; metadata:Record<string,unknown> }
export interface ProjectCandidate { project_id:string; work:DiscoveredWork; status:CandidateStatus; relevance_reason:string; relevance_tags:string[]; evidence_level:EvidenceLevel; discovered_by_search_run_id:string|null; discovered_by_conversation_id:string|null; import_task_id:string|null; paper_id:string|null; created_at:string; updated_at:string }
export interface ProviderStatus { state:"completed"|"failed"|"cancelled"; hits:number; error:string|null }
export interface LiteratureSearchRun { id:string; project_id:string; conversation_id:string; message_id:string; trigger:ResearchTrigger; question:string; query_plan:Record<string,unknown>; state:SearchRunState; provider_status:Record<string,ProviderStatus>; error:string|null; created_at:string; updated_at:string }
export interface LiteratureSearchResult { search_run_id:string; work:DiscoveredWork; providers:string[]; best_rank:number|null; provider_scores:Record<string,unknown>; raw_results:unknown[]; created_at:string }
export interface LiteratureSearchDetail { run:LiteratureSearchRun; results:LiteratureSearchResult[] }
export interface CandidateCitation { id:string; message_id?:string; project_id?:string; work_id:string; title:string; source_url:string; evidence_level:EvidenceLevel; quote:string; explanation:string; created_at?:string }
export type ImportCandidateOutcome={state:"already_in_project"|"linked_existing";paper_id:string}|{state:"enqueued";task_id:string}
export interface Annotation { id:string; citation_id:string; paper_id:string; revision:string; source_message_id:string; kind:string; body:string; state:"visible"|"hidden"; availability:"available"|"revision-stale"|"paper-missing"; created_at:string; updated_at:string }
export interface AnnotationAnchor { annotation_id:string; page:number; rect_index:number; x:number; y:number; width:number; height:number }
export interface PaperAnnotation { annotation:Annotation; citation:MessageCitation; anchors:AnnotationAnchor[] }
export interface ChatMessage { id:string; conversation_id:string; role:"user"|"assistant"|"system"; content:string; live_content?:string; turn_id:string|null; status:string; error:string|null; research_mode:ResearchMode; citations:MessageCitation[]; candidate_citations:CandidateCitation[]; progress_phase?:"reading"|"reasoning"|"tool"|"answering"|ResearchProgressPhase; progress_label?:string; created_at:string; updated_at:string }
export interface ConversationDetail { conversation:Conversation; scopes:ConversationScope[]; messages:ChatMessage[] }
export interface ConversationStreamEvent { id:number; type:string; conversation_id:string; message_id:string|null; payload:Record<string,unknown>; created_at:string }
