# Audit d'integrita': risultati e copertura

Baseline: `b9660013573bb45409d9b536c59ad8895701d626`, 2026-10-16.
La richiesta successiva autorizza correzioni e poi prove reali, in questo ordine.
Non e' ancora un audit conclusivo ne' una certificazione della release.

## Evidenze della review

- 831 file tracciati inventariati; dipendenze dei 15 crate e workspace Tauri separato ricostruite con Cargo metadata.
- 61 moduli frontend analizzati per import: 59 raggiungibili, residui `core/run.ts` e `ui/sidebar.ts`.
- 660 test riportati passati nelle suite selezionate, 2 ignorati. Non eseguita l'intera suite non ermetica.
- Angular: 138 test in 22 file; browser bridge Node: 2; Tauri: 5; ACP shared workspace: 1.
- Core selezionato: 385; tools/verify/eval/models: 114; store integration: 5; observe integration: 10.
- Build Angular riuscita: initial 1,16 MB contro budget warning 1 MB.
- Gate fmt fallito; Clippy fallito per `too_many_arguments` in `workspace_interact`.
- Check documentale baseline riuscito: 124 Markdown, 326 link.
- Demo visiva isolata: dark/light 1360x880, dark 640x480 e popup modello senza overflow orizzontale nei controlli effettuati. Nessuna prova nativa dedotta dalla demo. Server temporaneo fermato.
- Nessuna inferenza, installazione o download eseguiti durante la review.

## Registro dei rilievi

Stato iniziale: aperti salvo diversa indicazione. Riprodotto significa prova isolata, non necessariamente end-to-end nativo.

| ID | Priorita' | Rilievo | Evidenza |
|---|---|---|---|
| HI-01 | P1 | Server site segue symlink fuori directory | riprodotto su site.rs reale: file sintetico esterno, HTTP 200 |
| HI-02 | P1 | PTY e servizi persistenti senza GitConfigGuard | statico: profilo consente config, guard solo in run_command_once |
| HI-03 | P1 | Retry browser sul tab utente dopo revoca | riprodotto: model-tab/reload, epoch revocato, user-tab/reload |
| HI-04 | P1 | Eventi ACP non isolati completamente per generazione/sessione | stale session resa dal reducer reale |
| HI-05 | P1 | Core kill immediato, nuovo core avviato prima del cleanup | statico; albero processi nativo da provare |
| HI-06 | P1 | PTY chiude solo figlio diretto | statico; sopravvivenza discendenti da provare |
| HI-07 | P1 | write_all sotto mutex globale terminali | statico; non-reader puo' bloccare chiusura e altri tab |
| HI-08 | P1 | Uscita app senza protezione dirty buffer | statico, buffer volatili; close documento invece protetto |
| HI-09 | P2 | Rename binari senza hash | statico, file_request binary non restituisce hash |
| HI-10 | P2 | Editor legge intero file prima del limite 1 MB | statico, polling ogni 2,5 s amplifica costo |
| HI-11 | P2 | WorkspaceInteract salta intent/receipt e parte del progress tracking | statico, evento workspace.interaction omesso dall'export tipizzato |
| HI-12 | P2 | SSE llama decodifica UTF-8 per chunk arbitrario | statico, carattere spezzato diventa errore |
| HI-13 | P2 | Deadline comando non copre drain finale | statico; discendenti possono conservare pipe |
| HI-14 | P2 | Servizi non drenano stdout/stderr fino allo stop | statico; possibile blocco prima del bind |
| HI-15 | P2 | Manager calcola KV16, runtime puo' usare KV8 | statico, local_models vs compute_context |
| HI-16 | P2 | held=0 promosso a 2048 token | statico; OOM non misurato |
| HI-17 | P2 | Manager GGUF conserva ref/formato MLX nelle azioni | statico; Windows non certificato |
| HI-18 | P2 | kill/lsof/discovery eseguibili Unix nei percorsi Windows | statico |
| HI-19 | P2 | Tab sintetico senza focus, Enter textarea invoca submit | riprodotto in jsdom |
| HI-20 | P2 | Snapshot richiede crypto.randomUUID della pagina | API assente riprodotta; HTTP nativo da provare |
| HI-21 | P2 | URL richiesto mostrato prima del commit, frame non distinto | evidenza forte; test WebKit necessario |
| HI-22 | P2 | Sync webview fallito memorizzato nella firma | statico; niente retry con geometria invariata |
| HI-23 | P2 | JSON step inserito in script HTML senza escaping contestuale | statico; delimitatore script nel testo |
| HI-24 | P2 | Profilo/wrapper/log headless condivisi tra invocazioni | statico; concorrenza da provare |
| HI-25 | P1 | Gate fmt e Clippy non passano | eseguiti sulla baseline pulita |
| HI-26 | P2 | CI non invoca test Tauri separati e browser Node | statico |
| HI-27 | P2 | read_file suggerisce extract_document assente nel catalogo compatto | statico; allegati PDF invece estratti automaticamente |
| HI-28 | P3 | Suite seleziona l'esito più recente per ID task senza verificare corpus/harness | ipotesi di ambiguità; corpus correnti hanno ID unici e il runner dedicato usa una nuova directory |
| HI-29 | P3 | Due moduli frontend fuori grafo prodotto | parser AST, non analisi di ogni export |
| HI-30 | P3 | Token editor/terminale e label modello non uniformi | statico e demo |
| HI-31 | P3 | Bundle oltre warning budget | build eseguita, rallentamento non misurato |
| HI-32 | P2 | PTY senza backpressure end-to-end | statico, saturazione non misurata |
| HI-33 | P2 | Estrazione PDF tronca/rifiuta uno stream che contiene la parola `endstream` | causa verificata staticamente; regressione per PDF valido con `/Length` diretto aggiunta e verde |
| HI-34 | P3 | Cache indice può riusare record obsoleti se un writer ripristina mtime e mantiene la dimensione | riprodotto: riscrittura stessa lunghezza + ripristino esplicito di `mtime` lascia `symbols` e hash vecchi |
| HI-35 | P1 | Repository Hugging Face può uscire dalla directory scelta durante il download | riprodotto staticamente: `../../outside` passava `validate()` e `PathBuf::join` spostava la destinazione fuori root; ora rifiutato |
| HI-36 | P3 | Salvataggio configurazione workspace non atomico | statico: `fs::write` poteva lasciare JSON troncato in caso d'interruzione; ora usa la scrittura atomica già impiegata dagli strumenti |
| HI-37 | P2 | Downloader seguiva un symlink usato come file `.part` | statico: `metadata` e `OpenOptions` seguivano il link; ora `symlink_metadata` rifiuta un partial che non sia file regolare |

Nessun P0 dimostrato. Non correggere ipotesi non confermate come se fossero bug.
La rinomina dei normali file testuali e gli allegati PDF sono collegati: le prime ipotesi generali sono state smentite dai call site.

## Architettura verificata staticamente

UI Angular -> AgentStore/bridge -> Tauri -> pwr serve JSON-RPC -> TurnRunner -> executor/converse -> runtime/provider oppure tool -> risultato/ACP -> store/UI.
Browser condiviso: stessa child webview e DOM; controllo per conversazione nello store; screenshot look_at/check_page in Chromium separato, non stessa sessione WebKit.
Terminale personale e processo agente distinti; policy profilo dal core, PTY nel desktop; run_command usa pipe e altro execution path.
SQLite WAL con append transazionale; snapshot/checkpoint; JSON profilo/config; wiki/index; localStorage layout/tema; buffer editor volatili.

## Piano di correzione e prove

1. Regressioni mirate e correzioni dei difetti confermati, partendo da authority e lifecycle.
2. Ricontrollo dei rilievi statici al confine proprietario, evitando falsi positivi e refactoring generici.
3. Gate locali completi pertinenti, con risultati negativi conservati.
4. Solo dopo le fix: task sintetici bug-fix e progetto nuovo in directory temporanee, con acceptance test fissati prima della run.
5. Modelli gia' installati, uno alla volta; thinking/assistant/altre opzioni variate singolarmente. Nessuna compilazione/test/modifica PWR durante inferenza.
6. Niente task riservati o evidenze sigillate; nessun push/tag/release/version bump. Non installare o scaricare modelli implicitamente.
7. Errori di prodotto: diagnosi, correzione, regressione e replica comparabile. Fallimento del modello non equivale automaticamente a bug dell'harness.

## Copertura residua

Review manuale non esaustiva dei 831 file. Approfonditi browser/terminal/lifecycle/IPC/editor e parti di runtime, tool, persistenza, verifica e contesto; altre implementazioni lette solo per simboli/import.
Da approfondire: moduli secondari orchestrator, run storico, ogni tool e combinazione, parser PDF completo, retrieval, config/strategie, corpus e runner. Windows, E2E Tauri, crash/restart, cookie/dispositivi, tutti i controlli UI e inferenza sono ancora non verificati.

## Correzioni verificate finora

- HI-01: root canonica e controllo anche sul fallback; regressione symlink rossa poi verde, route SPA verde. Rimane la race tra verifica del percorso e apertura, non dichiarata risolta.
- HI-02: policy persistente con configurazione/hook Git protetti; test Seatbelt ha riprodotto la modifica, poi l'ha impedita. Contratto terminale core verde. Full access resta esplicitamente unconfined.
- HI-03: tab immutabile e rivalidazione prima di ogni retry; 5 regressioni rosse poi verdi, suite browser 11/11.
- HI-04: generazione in ACP/log e filtro sessione per timeline/usage; `resetConversationState` invalida callback, queue, permessi e risposte asincrone obsolete; 30 test AgentStore verdi.
- HI-05: EOF cooperativo fino a 3 s, stop precedente prima del nuovo core, gruppo Unix posseduto; test EOF nativo verde. Discendenti staccati e Windows richiedono altre prove.
- HI-07: writer unico per terminale, coda bounded, registro globale libero; test ordine/limite/indipendenza verde.
- HI-08: conferma nativa/frontend prima dell'uscita con draft, anche in workspace precedenti; test store verde. Crash recovery non aggiunto e chiusura UI nativa ancora da esercitare.
- HI-09/10: stat/hash streaming per rename binari e preview fisicamente bounded; 4 test backend editor, test prefix UTF-8 e 7 test store verdi.
- HI-11: eventi tool.action esportabili e intent/receipt per start/input; 6 test shared workspace e 3 product tools verdi. Progress tracking DOM resta da progettare/verificare, non abilitato arbitrariamente.
- HI-12: SSE byte-oriented con troncamento esplicito; 5 test SSE/HTTP verdi.
- HI-13: deadline estesa al drain e abort dei task I/O; 9 harness torture verdi, incluso figlio con pipe aperta.
- HI-14: drain concorrente e bounded fin dall'avvio; test servizio verboso rosso poi verde, 11 servizi verdi.
- HI-15/16/17: cache bits condivisi, ammissione sotto minimo rifiutata, ref/formato reali nelle azioni; test admission e 6 Model Manager verdi. Nessuna certificazione runtime Windows.
- HI-19/20: nonce host, Tab, Enter textarea e preventDefault; 5 Node bridge verdi.
- HI-21/22: URL letto dalla webview al load completato e retry bounded del sync fallito; compilazione Tauri e test occlusione verdi; test nativo iframe/recovery ancora necessario.
- HI-23/24: escaping HTML del JSON e directory per cattura con cleanup; test payload verde e fixture headless reale form/screenshot verde.
- HI-25/26: Clippy workspace/all-targets e fmt verdi; gate Tauri e Node aggiunti in CI. L'esecuzione hosted CI non e' stata interrogata.
- HI-27: consiglio PDF condizionato ai tool effettivamente offerti, alternativa allegato esplicita; 7 test documenti e product tools verdi.

La campagna con modelli e' iniziata dopo le correzioni e i gate automatizzati; la copertura reale e' limitata alle prove registrate in coda.

## Avanzamento 2026-10-17

- Corretto HI-04: `resetConversationState` invalida code, permessi, turni, stato del run e risposte asincrone obsolete durante cambio workspace, nuova conversazione e resume. Refresh contesto, compact, rewind, review e comandi ignorano risposte arrivate dopo la sostituzione della sessione. Aggiunte 3 regressioni mirate.
- L'avvio nativo iniziale si fermava durante la lettura della configurazione del workspace Desktop ricordato. Dopo aver riaperto lo stesso workspace tramite il selettore nativo, l'avvio e' riuscito; anche il bundle sorgente finale si avvia pronto. Evidenza compatibile con accesso/cartella macOS non ancora concesso alla copia locale; non si generalizza a una regressione del core.
- Le letture iniziali di modelli, sessioni e permessi ora registrano l'avanzamento nel core log dell'app; test aggiornato per verificare che `initialize` resti registrato anche dopo le letture.
- Gate completati: `cargo fmt --all -- --check`, `git diff --check`, `cargo test --workspace --no-fail-fast` (tutti i target verdi; 2 test reali llama/GGUF ignorati), `cargo clippy --workspace --all-targets -- -D warnings`, test desktop 151/151 in 22 file, browser bridge Node 5/5, test Tauri 8/8, check documenti 125 Markdown/326 link e build bundle macOS riuscita.
- Build Angular mantiene l'avviso bundle iniziale 1,16 MB contro budget 1 MB (circa +164 KB); la build Tauri non e' stata notarizzata per assenza di credenziali Apple, senza pubblicazione.
- Prova manuale locale con Gemma 4 26B A4B QAT 4-bit: primo turno completato in 32 s; secondo turno ha letto tre file con gli strumenti workspace, ha riportato endpoint e validazioni e terminato dopo 3 azioni in 15 s. Entrambi i prompt vietavano rete, shell e modifiche; nessun file del workspace e' stato modificato dalla prova.
- Nessun download/installazione modelli, push, tag o release. Restano da fare piu' modelli/opzioni, test E2E Tauri, Windows, crash/restart e le aree non ancora coperte elencate sopra.

## Avanzamento 2026-10-17 — passaggio successivo

- HI-33: il percorso statico mostra che la scansione per sottostringa si ferma su `endstream` dentro un payload e può rifiutare il documento. Il parser ora usa `/Length` diretto quando la posizione risultante coincide con il terminatore e mantiene il fallback esistente per lunghezze indirette/non valide. Regressione mirata verde; non è stato eseguito il test sulla versione precedente.
- Suite completa `cargo test -p pwr-tools`: 71 unit, 55 adversarial, 7 documenti, 6 fetch, 40 filesystem/VCS, 9 harness, 5 local service, 64 sandbox/approvals, 9 provisioning, 11 services e 2 skip-report verdi; 2 test non ermetici ignorati. Il test dotnet reale ha impiegato 127 s.
- `cargo test -p pwr-repo`: unit, gitignore, graph, retrieval e symbols verdi (42 test). `cargo test -p pwr-eval --test suites`: 2/2 verdi.
- HI-34: riprodotto su macOS con un file Rust riscritto a pari dimensione e `mtime` ripristinato via `File::set_times`; il secondo indice riusa il record `alpha` dopo che i byte contengono `bravo`. È un limite della cache basata su metadati, rilevante se uno strumento ripristina deliberatamente i timestamp o preserva metadati durante una modifica. Non corretto: rilevare questa condizione in modo portabile richiede una diversa scelta tra impronta nativa per piattaforma e rilettura/hash dei file; ridotto da P2 a P3 perché una normale riscrittura aggiorna `mtime` e viene riletta. I commenti che attribuivano all'hash una verifica sul cache hit sono stati corretti.
- HI-28: `find_outcome` seleziona per solo `task_id` e `completed_at`, mentre `TaskCase` contiene anche `corpus`. I nove corpus presenti hanno 80 ID unici e `suites/run.sh` crea una directory nuova; non è stata osservata contaminazione. Resta da chiarire il contratto quando chi invoca `pwr eval suite` riusa una directory con run di harness/corpus diversi; non corretto senza un caso d'uso confermato.

## Avanzamento 2026-10-17 — catalogo modelli

- HI-35: `ModelArtifact::validate` verificava i path dei file ma solo la presenza di `/` nel repository. Un registry workspace-local con repository `../../outside` faceva costruire destinazioni fuori dalla root selezionata. La validazione richiede ora esattamente `owner/name`, rifiuta slash inversi, path assoluti/traversal e file path con separatori Windows ambigui.
- `cargo test -p pwr-domain`: 71 test verdi, incluso il catalogo Gemma distribuito. `cargo test -p pwr-models`: 13/13 verdi. I due test regressione per repository e file path non sicuri sono inclusi nella suite di dominio.
- `cargo check -p pwr-cli` e `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml` verdi dopo la modifica del contratto di validazione. Non ho ricompilato il bundle desktop in questo passaggio.
- `cargo fmt --all -- --check` e `git diff --check` verdi.
- Follow-up statico: letti cache/index/retrieval e call site in CLI/context/wiki; letti catena SQLite, restore/resume/delete di conversazioni, intent/receipt e reconciliation. Non è emerso un ulteriore difetto confermato in queste superfici. Le suite store/orchestrator non sono state rilanciate perché questi moduli non sono stati modificati in questo passaggio.

## Avanzamento 2026-10-17 — cache e configurazione

- HI-34 riprodotto su macOS con `File::set_times`: dopo riscrittura a pari lunghezza e ripristino del timestamp, `index_incremental` riusa `symbols` e hash precedenti. Severità ridotta a P3 per il caso di metadati preservati intenzionalmente; corretti i commenti che promettevano erroneamente verifica via hash. `cargo test -p pwr-repo --test graph`: 8/8 verdi.
- HI-36: `save_chat_config` ora usa `pwr_tools::atomic::write_atomic`, evitando un file di configurazione parziale in caso d'interruzione. `cargo test -p pwr-cli saved_gemma_model_remains_selected`: test mirato verde; la primitiva atomica ha inoltre i propri test d'interruzione nel modulo `pwr-tools` già eseguiti nel passaggio precedente.
- `cargo fmt --all -- --check` e `git diff --check` verdi dopo le modifiche.
- Rilettura del parser PDF: `/Length` indiretto resta fuori dal parser strutturale e usa il fallback sul marker `endstream`; è un limite già dichiarato, non esteso in questo passaggio.
- HI-37: il downloader ora controlla il file parziale con `symlink_metadata` in preflight e prima di leggerlo o aprirlo per la ripresa; un symlink preesistente viene rifiutato come conflitto. Regressione Unix: il target esterno resta byte-per-byte invariato. `cargo test -p pwr-models`: 14 test verdi; restano non provate le sostituzioni concorrenti del path tra controllo e apertura.
- `cargo test -p pwr-cli --no-fail-fast`: 264 unit, 2 build identity, 1 readiness, 12 routing e 2 sessioni verdi.
- Verifica finale ripetuta: `cargo fmt --all -- --check`, `git diff --check` e `python3 scripts/check-public-docs.py` verdi (125 Markdown, 326 link locali).
- Build pacchetto desktop macOS completata con `npx tauri build --bundles app --no-sign --ci`; bundle di 62 MB creato in `/tmp/pwr-integrity-20261017/release/bundle/macos/PWR.app`, con eseguibili e risorse inclusi. Build senza firma Apple; non equivale a notarizzazione o test di avvio. L’istanza PWR gia' aperta non e' stata chiusa o sostituita.
- Copertura residua confermata: Windows e WebKit nativo non disponibili per una prova runtime completa in questa sessione; due test live llama/GGUF restano ignorati. Restano inoltre non provati crash/restart, saturazione del trasporto PTY e tracking del progresso DOM; HI-28 resta un contratto da chiarire, mentre HI-34 e' un limite noto della cache a metadati. La campagna modelli riportata sopra copre solo Gemma 4 26B A4B QAT 4-bit e due prompt di prova.
