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
