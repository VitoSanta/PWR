# Rapporto sprint S03 — Controller, modifiche e contesto

Collegato alla [scaletta 0.5](../plan/sprint-v0.5.0.md). Stato: **DONE con residui**
(avvio e chiusura 2026-10-20, su decisione del proprietario). Nessuna inferenza eseguita in questa sessione; nessun gate
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

## 3. Verifiche eseguite

| Tipo | Comando | Esito |
|---|---|---|
| Regressione S03-01/02/03 | adversarial (57), `an_operation_inside…` (1), `delete_after_shrink` (2) | verdi |
| Crate complete | `cargo test -p pwr-orchestrator -p pwr-tools` (dopo S03-03) | nessun fallimento |
| Statica | `cargo clippy -p pwr-tools -p pwr-orchestrator --all-targets`; `cargo fmt` | pulito |

Suite Rust completa non eseguita (solo a rilascio). Inferenze: nessuna.

## 4. Decisione del proprietario (2026-10-20)

Opzione (b) approvata: dopo il rifiuto di una riscrittura totale, `delete_path` sullo stesso file nello stesso turno è rifiutato. Applicata come S03-03. Una riscrittura davvero voluta passa dal chiedere all'ingegnere o da un turno successivo.

## 5. Non ancora fatto in S03

- CLI-05: ora misurato (S03-04); resta da decidere un tetto di tempo per la review dopo le prime misure reali. Le interruzioni 26B/31B erano Stop dell'operatore durante la verifica di chiusura, non un timeout del core.
- Preflight del prompt effettivo e conservazione dei requisiti in compaction/resume.
- Limiti aggregati di turni e goal, Stop durante review.
- Misure prima/dopo con stessi pack/task/budget.

## 6. Decisione di chiusura

- Criteri soddisfatti: classificazione delle tracce; quattro correzioni con
  regressione e controesempio (S03-01..04). Nessuna inferenza, nessun gate.
- Criteri **non** soddisfatti, rinviati: preflight del prompt effettivo;
  compaction/resume che conservano requisiti e riferimenti ai file; limiti
  aggregati e Stop in review; misure prima/dopo con stessi pack/task/budget;
  tetto di tempo della review (serve prima una misura reale con S03-04).
- Chiuso dal proprietario per esigenze di calendario (build per il post del
  2026-10-20): i residui non sono «fatti» e restano a carico di S04/S07.
- Effetto sui gate: nessuno. Impatto Windows/release: nessuno nuovo.
