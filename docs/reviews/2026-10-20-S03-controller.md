# Rapporto sprint S03 — Controller, modifiche e contesto

Collegato alla [scaletta 0.5](../plan/sprint-v0.5.0.md). Stato: **IN_PROGRESS — tutti i punti implementati e provati con test;
resta la misura prima/dopo su modello reale** (2026-10-20). Nessuna inferenza eseguita in questa sessione; nessun gate
promosso; nessun default cambiato.

- Baseline: working tree dopo S02 (commit `06726c26` più modifiche S01/S02 non
  ancora committate). Sorgente di partenza delle tracce: cinque conversazioni
  del collaudo del sito (2026-10-19), evidenza privata `website-manual-20261019`.
- Dipendenze: S02 DONE. La qualificazione dei modelli resta da completare e
  non è chiusa da questo sprint.

## 1. Classificazione delle tracce (punto 1 della scaletta)

Cinque tracce (12B, 26B, 31B, E2B, E4B), 299 eventi non-generazione.

| Classe | Dove | Osservato | Stato del controller | Azione S03 |
|---|---|---|---|---|
| Argv con programma ripetuto | 26B: `mkdir mkdir docs`, `mv mv …`, `lsof lsof`, `cat cat`, `kill kill`; E4B `ng ng new` | Il livello tool già scarta la ripetizione iniziale: `command_line()` mostra ciò che gira davvero | Coperto (S-prima); la riga di dettaglio ACP mostra l'argv proposto, non quello eseguito | Nessuna modifica |
| Intera riga di comando in un solo elemento di args | E4B (WEB-06): `ng` con `["ng new site --style=scss …"]`, due volte, poi stallo | Arrivava al programma come unico argomento ignoto; CLI-07 copriva solo Python/Node | **Lacuna** | S03-01: rifiuto con argv da riproporre per `ng npm npx pnpm yarn cargo git uv pip pip3` |
| Campo obbligatorio nel posto sbagliato | 26B (WEB-08): `operation` dentro `arguments`, 13 rifiuti `missing field operation`; il modello lo riconosce nel ragionamento e lo ripete | Feedback generico (elenco delle chiavi), nessun recupero | **Lacuna** | S03-02: lettura unica in `repair_form` |
| Scrittura identica ripetuta (no-op) | 12B (WEB-07): 7 `write_file` su `app.ts`, poi proposta già rifiutata | Protezione corretta; arresto `recovery` dopo 3 finestre da 6 azioni | Funziona come progettato; la causa (build rotto altrove) non è risolta dal controller | Nessuna modifica: il recupero richiede lettura→patch mirata del modello |
| Ripetizione del testo / risposte inutilizzabili | 12B: 3 stop del motore per ripetizione prima di qualunque azione | `protocol` dopo tre risposte inutilizzabili | Funziona; dipende dal modello/motore (S04) | Passa a S04 |
| Riscrittura totale rifiutata, poi `delete_path`+`write_file` | 26B call 52–54 e 73–75 | Il modello ha aggirato da sé il rifiuto «351 righe cancellate»; il messaggio stesso suggeriva la via | Lacuna, decisa dal proprietario (§4) | S03-03: `delete_path` rifiutato nello stesso turno |
| Verifica indipendente interrotta | 26B, 31B: `interrupted`, «changes kept and remain unverified» | Onesto; costo/tempo della review non separato (CLI-05) | Aperto | Prossimo passo |
| Completamento senza esecuzione, nessun check dichiarato | E2B: 18 file scritti, nessun comando, «no checks were run» | Onesto («not verified») | Corretto | Nessuna modifica |
| Check-in senza azioni | E4B: stallo `stalled` dopo 3 check-in | Funziona; era conseguenza di WEB-06 | Atteso migliorare con S03-01 | Da misurare |

## 2. Ticket e criteri

| ID | Problema | Criterio prefissato | Implementazione | Evidenza | Residuo |
|---|---|---|---|---|---|
| S03-01 | WEB-06: riga intera in `args[0]` | `npm ["npm run build --silent"]` rifiutato, nulla eseguito, argv suggerito; `echo ["echo is a word"]` e argv separato invariati | `pwr-tools/src/lib.rs` (rifiuto con esempio, mai split silenzioso) | `adversarial.rs::a_whole_command_line_in_one_arg_is_refused_with_the_argv_to_resubmit` verde | Non copre ogni forma di riga malformata; elenco di programmi volutamente chiuso |
| S03-02 | WEB-08: `operation` annidata | `{"arguments":{"operation":"read",…},"target":"terminal"}` → `operation` al livello alto; un `operation` già presente vince; valore non stringa e altri tool intatti | `pwr-orchestrator/src/lib.rs::repair_form` | `an_operation_inside_arguments_is_read_beside_the_target` verde | Beneficio sul modello reale **non misurato**: serve replica 26B con stesso task/budget |
| S03-03 | Aggiramento del rifiuto di riscrittura totale | Dopo un `write_file` rifiutato come riscrittura totale, `delete_path` sullo stesso file nello stesso turno è rifiutato con la via corretta (`replace_text`/patch); file intatto; senza rifiuto precedente la cancellazione funziona | `converse.rs` (`shrink_refused` per turno) e messaggio in `pwr-tools` senza più il suggerimento di cancellare | `tests/delete_after_shrink.rs` (2 test) verdi | Un turno successivo riparte pulito; il beneficio sul modello reale non è misurato |
| S03-04 | CLI-05: tempo di verifica e review non misurato | Il budget del goal riporta `verification_seconds` e `review_seconds`; i limiti non cambiano | `executor.rs` (`GoalBudget`, `snapshot`), esposto in `goalBudget` | `the_budget_snapshot_reports_time_spent_verifying_and_reviewing`; 22 test `goal` e 9 `goal_budget` (cli) verdi | Solo misura: serve una run reale per vedere il costo; nessuna decisione sul tetto di tempo della review |
| S03-05 | Preflight: i tool schema non erano contati prima del primo conteggio del backend | Una cronologia che sta sotto il trigger di compaction solo senza lo schema viene compattata prima della prima richiesta; senza schema non cambia nulla; finestre ≤ 8.192 token invariate; il seme non supera un quarto della finestra | `PromptOverhead::seeded_with_schema` (`context.rs`), usato in `converse.rs` | `tests/schema_preflight.rs`, `prompt_overhead_tests` (3); un test `pwr-cli` ha dovuto alzare il ceiling a 8.192 perché 4.096 era sotto il costo dello schema (~3.300 token) | Il vantaggio è sul primo turno / dopo un resume; dopo il primo conteggio il backend resta l'unica correzione |
| S03-06 | Stato terminale `recovery`/`protocol` nei runner | Una prova che termina in `recovery`/`protocol` non è mai un passaggio pulito, anche se il checker esterno passa; conta solo l'ultimo turno | `evidence/stack-matrix/runner/analyze.py` (`terminal_failures`, `clean_final`); il runner Rust già classifica `Recovery`/`Protocol` come non risolti | `test_analyze.py` (2 test), 29 test Python del runner | `passed` resta il verdetto esterno; il conteggio è a parte |
| S03-07 | Limiti aggregati, cicli improduttivi, tempo in review | fix→test rosso→fix con stati nuovi non è mai fermato; lo stesso ciclo fra due stati già visti sì; il tempo di una review reale finisce nel budget | test in `run_state.rs` e `executor.rs` | `fix_red_test_fix_is_progress…`, `the_time_a_goal_spends_in_its_review_is_in_its_budget` verdi; esistevano già i test sui tetti (`goal_budget_*`, `BudgetSpent`, Stop in review) | Nessun tetto di tempo per la review: serve prima la misura reale |


## 3. Verifiche eseguite

| Tipo | Comando | Esito |
|---|---|---|
| Regressione S03-01/02/03 | adversarial (57), `an_operation_inside…` (1), `delete_after_shrink` (2) | verdi |
| Crate complete | `cargo test -p pwr-orchestrator -p pwr-tools -p pwr-cli -p pwr-domain -p pwr-verify -p pwr-eval`; test Python del runner | nessun fallimento |
| Statica | `cargo clippy -p pwr-tools -p pwr-orchestrator --all-targets`; `cargo fmt` | pulito |

Suite Rust completa non eseguita (solo a rilascio). Inferenze: nessuna.

## 4. Decisione del proprietario (2026-10-20)

Opzione (b) approvata: dopo il rifiuto di una riscrittura totale, `delete_path` sullo stesso file nello stesso turno è rifiutato. Applicata come S03-03. Una riscrittura davvero voluta passa dal chiedere all'ingegnere o da un turno successivo.

## 5. Stato dei punti della scaletta

1. Classificazione delle tracce: fatta (§1).
2. Recupero deterministico dopo conflitto: nessuna cancellazione+riscrittura automatica; S03-03 la vieta anche al modello nello stesso turno. La rilettura è già imposta da «not read first».
3. Limiti aggregati, Stop, tempo in review, cicli improduttivi: S03-04 e S03-07.
4. Preflight del prompt effettivo: S03-05 (schema); obiettivo, policy, memoria e sorgenti erano già nel compilatore del contesto (`context_compiler`, `compaction_fidelity` verdi).
5. Compaction/resume e stati terminali: già coperti da `goal_retention`, `compaction_fidelity`, `conversation_continuity`; stati terminali nei runner: S03-06.
6. Misura prima/dopo con modello reale: **vedi §6**.

## 6. Misura prima/dopo

Protocollo: 26B A4B 4-bit, fixing e creazione come in `cli-usability-20261019`, 2 ripetizioni per braccio e per task, bracci alternati, campionamento non seeded, 300 s per richiesta, checker esterno congelato. «Prima» = binario congelato `recovery-r1` (19 ottobre); «dopo» = core incluso nel pacchetto 0.4.1-alpha costruito dal commit di S03. La differenza è cumulativa S01–S03, non isolabile a S03; con 2 ripetizioni non è una stima di probabilità di successo. Esiti aggiunti qui sotto quando disponibili.
