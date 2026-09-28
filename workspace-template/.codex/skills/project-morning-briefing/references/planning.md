# Adaptive research map

Read the project purpose, research questions, tree, available READMEs, explicit preferences, and existing work together. Folder names alone are insufficient to infer every scope. Trace meanings through parents and descendants; do not import interests from unrelated projects.

Form a small set of useful semantic themes, not one query per folder. Merge overlapping or organizational branches where their research questions coincide; split a broad node only when its questions genuinely differ. Prefer a few interpretable themes over many near-duplicate aliases. Choose priorities from explicit project focus, not paper volume. When priorities are unspecified, use equal priority.

Every supplied project node must either appear in at least one theme's project_ids or have a disposition explaining why it is organizational, needs clarification, or is deferred. A root or grouping folder can be organizational. A catch-all folder can support exploratory scope if its contents or purpose justify it; its name alone is not a research topic. Unknown abbreviations can retain their literal search term as a provisional theme only when that remains meaningful, with uncertainty recorded. Do not silently drop an explicit direction.

Output JSON only:

```json
{
  "rationale": "How the map follows the project and why topics were merged or split",
  "topics": [
    {
      "id": "stable-short-id",
      "label": "Reader-facing theme or research question",
      "project_ids": ["actual-project-id"],
      "intent": "What belongs here; distinguish adjacent themes",
      "priority": 2,
      "terms": ["specific research phrase", "alternative established phrase"]
    }
  ],
  "dispositions": [
    {"project_id": "actual-group-id", "kind": "organizational", "reason": "This node groups the researched descendants"}
  ]
}
```

Operational bounds: 1–12 themes; 1–5 short English search phrases per theme; priorities 1–3; topic ids use lowercase letters, digits and hyphens. A term is 3–100 bytes and uses English letters, digits, spaces, hyphens, underscores or periods, without query operators. disposition kind is organizational, clarification, or deferred; explanations are required. Use only supplied project ids. If the project is too ambiguous to form even one grounded theme, report that problem rather than manufacturing a research map.

Use genuine alternative terminology, including established acronyms when meaningful. Do not stuff several increasingly long versions of the same phrase into the plan to increase its apparent importance. Supplementary interests help refine relevant themes, not override the project. A newly added or renamed branch can change the map: previous plans are a cache, not authority.

The application searches themes with a bounded shared request budget, records retrieval independently, and performs at most one shared category review when coverage needs checking. It ranks aliases by their strongest match, not cumulative synonym counts, and selects for useful thematic coverage before filling spare places. Do not pre-empt those mechanics by trying to search or inventing results during planning.
