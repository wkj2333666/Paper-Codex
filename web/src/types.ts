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
export interface CodexCapabilities { default:CodexRunSettings; models:CodexModel[] }
export interface Conversation { id:string; title:string; thread_id:string|null; status:string; model:string|null; reasoning_effort:string|null; service_tier:string|null; archived_at:string|null; created_at:string; updated_at:string }
export interface ConversationScope { conversation_id?:string; scope_type:"paper"|"project"|"global"; scope_id:string|null; added_at?:string }
export interface MessageCitation { id:string; message_id:string; paper_id:string; revision:string; page:number; section:string|null; locator:string|null; quote:string; prefix:string; suffix:string; explanation:string; match_status:string }
export interface Annotation { id:string; citation_id:string; paper_id:string; revision:string; source_message_id:string; kind:string; body:string; state:"visible"|"hidden"; availability:"available"|"revision-stale"|"paper-missing"; created_at:string; updated_at:string }
export interface AnnotationAnchor { annotation_id:string; page:number; rect_index:number; x:number; y:number; width:number; height:number }
export interface PaperAnnotation { annotation:Annotation; citation:MessageCitation; anchors:AnnotationAnchor[] }
export interface ChatMessage { id:string; conversation_id:string; role:"user"|"assistant"|"system"; content:string; turn_id:string|null; status:string; error:string|null; citations:MessageCitation[]; progress_phase?:"reading"|"reasoning"; created_at:string; updated_at:string }
export interface ConversationDetail { conversation:Conversation; scopes:ConversationScope[]; messages:ChatMessage[] }
export interface ConversationStreamEvent { id:number; type:string; conversation_id:string; message_id:string|null; payload:Record<string,unknown>; created_at:string }
