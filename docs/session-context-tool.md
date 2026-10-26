# On-Demand Session Context

`ReadSessionContext` lets the agent use context from another persisted ZCode session only when a current turn actually needs it.

## Goals

- Avoid pre-generating capsules for every session.
- Avoid injecting another full session into the current model context.
- Keep the feature in the agent layer so desktop, web, and remote clients share the same behavior.
- Make `#sess_*` references lightweight UI syntax, not a protocol requirement.

## Flow

1. The current user prompt may mention a prior session, for example `#sess_abc`.
2. The runtime injects a small current-turn reminder that the referenced session is available through `ReadSessionContext`.
3. The model calls `ReadSessionContext` only if it needs that history.
4. The tool reads the target session from `SessionStorePort`.
5. The tool cleans persisted messages, removes model-only/internal reminder content, and builds bounded transcript chunks.
6. If a lite model is available, the tool asks it to extract relevant or handoff context from bounded material.
7. If lite extraction is unavailable or fails, the tool returns bounded local snippets instead.

## Extraction model

The extraction step runs on the session's auxiliary model, falling back to the calling turn's model. The runtime owns the choice; the tool only reads `context.getAuxiliaryModel?.() ?? context.model`.

The auxiliary model is the model the built-in `Explore` subagent would run on, resolved lazily on first use within a tool call:

| Condition                                                                                                                                                        | Model used                                                    |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| The active `Explore` profile has an explicit `modelSelection` (built-in override from Settings, or a user/project profile named `Explore`) that resolves cleanly | That Explore model                                            |
| No explicit Explore selection (Explore inherits the parent)                                                                                                      | Turn model                                                    |
| The Explore selection fails to resolve (missing account, provider, model, or reasoning level)                                                                    | Turn model; logged at `warn`, the tool does not fail          |
| The turn carries a Core Server `subagentModelOverride`, or an off-peak execution is active                                                                       | Turn model (those paths already pin which model children use) |
| Explore selection equals the turn model's provider/model                                                                                                         | Turn model                                                    |

Auxiliary calls still use `auxiliaryModelOptions()` (lowest reasoning level). If the Explore model call itself fails, the tool returns bounded local snippets as before; it does not retry on the turn model.

Acceptance:

- With an Explore override set, ReadSessionContext's `read_session_context_extract` requests go to the Explore provider/model.
- Without one, requests go to the turn model, unchanged from before.
- An unresolvable Explore selection never fails the tool.
- Tools that do not call `getAuxiliaryModel()` never construct the auxiliary model.

## Timeout

`ReadSessionContext` has a fixed five-minute tool execution timeout. The timeout is not call-overridable because the tool is read-only but may need to scan a large persisted session and run lite-model extraction before returning bounded context.

This timeout is owned by the agent tool layer, so desktop, web, and remote clients share the same behavior.

## Strategies

- `relevant`: focused retrieval for a concrete question or current task.
- `handoff`: bounded continuation context with objective, decisions, files, blockers, verification, and next steps.

The returned content is background material. It does not override current user, developer, or system instructions.
