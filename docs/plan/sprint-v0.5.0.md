# PWR — Sprint verso la 0.5.0 e il collaudo finale Windows

Revisione: **2026-10-20**. Stato: **S00–S02 DONE, S03 DONE con residui; S04–S12 pianificati**.
Target: completare sviluppo macOS, validare fixing/creazione/uso generale,
qualificare Windows e preparare una possibile 0.5.0 pubblica. La 1.0 richiederà
anche esperienza d'uso esterna. **Aggiornamento del proprietario 2026-10-19:**
account Apple e qualificazione Developer ID/notarizzazione sono rinviati a S12,
dopo Windows, quando disponibili. Non bloccano sviluppo e collaudi ad-hoc. Il frontend è dichiarato concluso dal
proprietario: suite automatiche sul checkout consolidato completate; qualificazione nativa finale in S09.

Questa è la scaletta settimanale di esecuzione. Conserva i contratti del
[piano tecnico](implementation-plan.md), della [campagna Gemma/Windows](piano-campagne-gemma4-e-windows.md)
e di [MASTER_SPEC](../../MASTER_SPEC.md). Lo stato dei gate resta esclusivamente
in [milestones.json](../milestones.json). Uno sprint DONE non passa un gate.
Nessun bump, commit, push, tag, pubblicazione, acquisto o nuova integrazione
runtime viene eseguito dalla creazione di questa guida.

## Come organizzare il tempo

Una settimana di lavoro per sprint è l'unità iniziale di pianificazione,
non una promessa di durata. Sono **13 sprint, da S00 a S12**: circa tredici
settimane come prima ipotesi, più eventuali estensioni. Se un criterio di
uscita non passa, lo sprint continua o viene suddiviso: non si sposta il bug
nel collaudo finale per rispettare il calendario. Le prove F6 complete e
l'attivazione dei certificati possono richiedere tempo aggiuntivo.

Cadenza proposta: giorno 1 riproduzione e criteri; giorni 2–3 implementazione;
giorno 4 verifiche; giorno 5 revisione, documentazione e scelta del lavoro
successivo. Gli sprint di collaudo dedicano invece la settimana all'esecuzione
e al triage. Non iniziare un altro sprint funzionale mentre quello precedente
ha un difetto bloccante aperto.

Durante le inferenze locali: un modello alla volta e nessuna modifica,
compilazione o test PWR concorrente, neppure da un'altra chat. Prenotare
esplicitamente una finestra; controllare sorgenti e processi prima e dopo.
Separare sempre ambiente prodotto, workspace dei task e verificatore.

## Decisioni di perimetro e qualificazione

| Decisione | Proposta operativa | Confine |
|---|---|---|
| Release | Candidati `0.5.0-rc.N`, poi possibile `0.5.0`; `0.5.0-beta.N` se si vuole un canale pubblico sperimentale | Il nome definitivo è del proprietario; nessun manifest viene aggiornato ora |
| 0.5 e missione scientifica | Distinguere accettazione del prodotto beta da eventuale claim di superiorità dell'harness | F6 è già Gemma-only per D-2026-10-18-2; G3/F6 richiedono ancora confronti e soglie non soddisfatti. Per cambiare il contratto serve una decisione esplicita, non il numero 0.5 |
| Modelli e macchine | Indicare pack/versioni realmente supportati, macOS minimo, RAM, Windows/GPU/backend target | Un modello caricabile non è automaticamente qualificato per coding; niente esclusioni opportunistiche dopo i risultati |
| Soglie di qualità | Fissare prima delle prove numero task, repliche, budget per classe, margini, interventi ammessi e difetti bloccanti | I limiti del pilot Harness² non diventano soglie di prodotto |
| Apple | Rinviare iscrizione e Developer ID a S12, dopo Windows, quando disponibili | S08/S09 usano pacchetti ad-hoc; la qualificazione della distribuzione firmata resta distinta e aperta |
| Ricerca | Budget di uno sprint per studio/prototipi e uno per l'eventuale integrazione scelta | Nessuna ricerca illimitata; risultato legittimo anche «non adottare ora» |

**Vincolo da conservare:** con il contratto attuale Windows/release non
possono essere dichiarati pronti saltando G3/F6. In S00 si sceglie se mantenerlo
integralmente, estendendo S07 quanto serve, oppure proporre formalmente criteri
separati per una beta 0.5. Finché non approvata, l'eccezione non esiste.
Non si allentano sicurezza, integrità dei dati o onestà dei risultati.

## Sequenza e registro corrente

| Sprint | Obiettivo | Dipende da | Stato iniziale | Consegna principale |
|---|---|---|---|---|
| S00 | Consolidamento, baseline e contratto 0.5 | Frontend concluso | DONE | Identità candidata, perimetro e backlog riconciliato |
| S01 | Sicurezza, lifecycle e recupero | S00 | DONE | Difetti bloccanti riprodotti e chiusi |
| S02 | Verifica indipendente e correttezza | S01 | DONE | Risultati ancorati a prove e copertura esplicita |
| S03 | Controller, editing e contesto | S02 | DONE con residui | Meno loop e recuperi falliti, senza regressioni |
| S04 | Gemma e prestazioni locali | S03 | PLANNED | Matrice pack/runtime e profili misurati |
| S05 | Studio integrazioni agentiche | Baseline S04 | PLANNED | ADR e misure di fattibilità per ogni candidata |
| S06 | Integrazione selezionata o ottimizzazione mirata | Decisioni S05 | PLANNED | Una sola modifica principale misurabile e reversibile |
| S07 | Qualità coding e regressioni complete | S06 | PLANNED | Suite dev fixing/creazione/generale e disposizione G1/G2/G3/F6 |
| S08 | Distribuzione macOS ad-hoc e updater | S07 | PLANNED | Pacchetti ad-hoc verificati e upgrade reali; Apple differito |
| S09 | Collaudo finale approfondito macOS | S08 | PLANNED | Rapporto E2E sul candidato installato |
| S10 | Qualificazione tecnica Windows | S09; gate applicabili | PLANNED | Installer/backend/processi/confinamento verificati |
| S11 | Collaudo finale approfondito Windows | S10 | PLANNED | Stessa suite funzionale, esiti propri della piattaforma |
| S12 | Account/firma Apple quando disponibili, 0.5 e utenti | S09 + S11 + gate; Apple per distribuzione firmata | PLANNED | Qualificazione firmata finale, go/no-go e feedback |

Gli sprint S00–S08 terminano lo sviluppo funzionale macOS. S09 e S11 sono
collaudi: i bug riaprono lo sprint pertinente. In S10 sono ammesse le correzioni
di portabilità; una modifica condivisa impone una nuova verifica macOS della
parte interessata. Windows resta ultimo come richiesto.

## S00 — Consolidare e sapere esattamente cosa stiamo rilasciando

**Avvio 2026-10-19:** [baseline nuova e ticket](../reviews/2026-10-19-S00-baseline.md); suite automatiche riuscite e [37 finding instradati](../reviews/2026-10-19-S00-findings.md). Restano walk nativo e decisioni di perimetro: sprint IN_PROGRESS.

**Riscontro frontend 2026-10-19:** il proprietario accetta il frontend dopo
le correzioni e le verifiche mirate; [registro](../reviews/2026-10-19-S00-native-followup.md).
Chiusa l’iterazione UI corrente. Restano residui tecnici, consolidamento
del candidato e decisioni di perimetro; collaudo E2E finale in S09.

**Residui tecnici 2026-10-19:** [rapporto](../reviews/2026-10-19-S00-residuals.md); bozza del [contratto 0.5](acceptance-v0.5.0.md). La scelta Gemma-only e la revisione F6 sono già registrate; restano matrici e criteri numerici.

**Consolidamento 2026-10-20:** [rapporto e risultati](../reviews/2026-10-20-S00-closeout.md).
Inventario e suite del checkout corrente completati: 192 test frontend,
1.607 pass Rust riportati (6 ignored e un ritorno anticipato Docker), 18 test
native; sidecar, runner, release tooling, Clippy e build riusciti.
Windows identificato: Windows 11, Intel i7, 32 GB RAM, RTX 4080 16 GB VRAM;
qualificazione in S10. Il proprietario ha approvato il [perimetro](acceptance-v0.5.0.md) il 2026-10-20:
**S00 DONE**, nessuna deroga a G3/F6; decisione D-2026-10-20-1.
Nuova segnalazione: lettura dei PDF allegati non corretta.
Riprodotta il 2026-10-20 sulla guida del progetto: PDF valido con font
Identity-H e sette riferimenti ToUnicode; l'estrattore PWR supporta soltanto
codifiche semplici e rifiuta il testo. Messaggio inesatto corretto; supporto
Unicode ancora aperto. [Diagnosi e workaround](../reviews/2026-10-20-pdf-attachment.md).
Instradare in S01/S02 distinguendo
importazione, estrazione, eventuale OCR, consegna al contesto e uso da parte
del modello. Non presumere che sia la causa storica HI-33 né aggiungere OCR
come requisito implicito. Riproduzione via CLI dove copre il percorso;
il collegamento attachment/IPC richiede una verifica mirata dell'app.

**Lavoro tecnico**

- [x] Raccogliere diff e inventario frontend senza sovrascrivere altri lavori;
  accettazione del proprietario e suite consolidate registrate. Controlli nativi
  residui, tastiera e accessibilità assegnati a S01/S02/S09 secondo perimetro approvato.
- [x] Inventariare tutti i file modificati/non tracciati, dipendenze, versioni,
  feature flag, configurazioni e componenti nativi. Registrare strumenti/host.
- [x] Eseguire le suite del candidato: Rust workspace/native, frontend da
  installazione pulita, sidecar, runner e strumenti release. Registrare skip.
  Usare [testing](../testing.md) e CI correnti; i conteggi storici non valgono.
- [x] Ricondurre ogni finding HI/F1/W ancora aperto a ticket, test ed evidenza;
  distinguere implementato, misurato, non provato, bloccato e rinviato.
- [x] Approvare il perimetro S00 e identificare il target Windows; Apple differito
  a S12. Matrici/stack/pack e protocolli quantitativi da approvare prima delle
  rispettive campagne S04/S05/S07; qualificazione Windows S10.

**Dove:** `.github/workflows/`, `apps/desktop/`, `strategies/artifacts.json`,
`docs/release/next-release-readiness.md`, registro HI e `docs/milestones.json`.
**Uscita:** una baseline identificata, suite senza fallimenti inspiegati,
backlog senza finding orfani e nessun conflitto di release policy nascosto.
**Mappa:** F0/F1, W0, W7, W9.1. Non significa G1 passato.

## S01 — Effetti sicuri, processi e recupero

**Verifica finale 2026-10-20:** [rapporto e matrice](../reviews/2026-10-20-S01-closeout.md).
Implementati generazioni MLX isolate, pubblicazione atomica, cancel/commit
ordinati, blocco degli avvii dopo close e supervisione indipendente di core,
PTY e installer dopo SIGKILL. Walk isolato dell’app: trust, terminale, browser
senza IPC privilegiato, conflitto esterno, crash/restart con draft recuperato
e uscita normale riusciti; zero processi posseduti attivi dopo entrambe le uscite.
54 pass nativi (quattro entry point fixture), prova venv separata, 192 test frontend
e suite workspace/sidecar/runner/tooling riportate con skip nel rapporto.
Completati anche recupero legacy mediante reinstallazione isolata e pulizia offline
con anteprima/apply, lease ereditate e ripresa dopo SIGKILL. Materiale legacy e
ambienti sconosciuti conservati, non dichiarati cancellabili senza identità.
S01 **DONE**, con procedura e confini nel rapporto; nessun gate di release promosso.

**Nuove regressioni dal proprietario:** [audit website dei cinque modelli](../reviews/2026-10-19-website-manual-audit.md), WEB-01–WEB-14. Startup/lifecycle e aggiornamento dei tool precedono nuove prove lunghe; compilazione verde con scaffold non qualifica il sito.

- [x] Completare la matrice critica di browser, terminale, IPC, filesystem,
  download, servizi e autorizzazioni sul sorgente corrente.
- [x] Provare symlink/race, uscita dal workspace, pagina ostile, callback dopo
  chiusura, flood PTY, annullamento durante comando/download/installazione.
- [x] Verificare Stop, chiusura normale, crash core/app e riavvio: nessun processo
  orfano, draft conservati, conflitto con modifica esterna, database corrotto
  riconosciuto. Distinguere crash, power loss e recovery transazionale provati.
- [x] Chiudere il recupero di installazioni interrotte da terminazione forzata,
  se previsto dal contratto; in alternativa definire un recupero assistito
  sicuro e una limitazione accettata esplicitamente prima della release.
- [x] Aggiungere regressioni ai difetti riprodotti e ripetere il walk nativo
  dopo la correzione, conservando il primo risultato negativo.

**Dove:** `crates/pwr-tools/`, `crates/pwr-store/`, download di `pwr-models`,
`apps/desktop/src-tauri/src/{browser,terminal,engine,engine_install,drafts,updater}.rs`.
**Uscita:** zero difetti noti bloccanti di scrittura non autorizzata, perdita
dati, isolamento o processi; ogni limite residuo ha una disposizione scritta.
**Mappa:** G1, W1/W5/W6/W7, [recupero editor](editor-recovery.md),
[storage download](download-storage.md). Non ripetere fix già provati senza motivo.

## S02 — Rendere la verifica affidabile

**Avvio 2026-10-20:** [registro S02](../reviews/2026-10-20-S02-verification.md).
**Chiusura 2026-10-20: DONE per lo sviluppo macOS**, con
[matrice e validazione finale](../reviews/2026-10-20-S02-closeout.md).
Corretti exit 0 ingannevoli, omissioni di baseline e PDF-01; aggiunte prove
legate al sorgente e regressioni semantiche indipendenti. 1.280 test Rust,
194 frontend e test separato del PDF originale riusciti; nessuna inferenza.
[Scope e limiti delle prove](verification-evidence.md): nessuna qualificazione
universale dei modelli o del pacchetto, nessun gate promosso.

- [x] Trasformare i difetti Unicode/precisione/bool/atomicità già osservati in
  regressioni dev, senza contaminare i task futuri o cambiare test sigillati.
- [x] Legare ogni risultato a hash dei file, comandi, baseline, test eseguiti,
  scope e modifiche successive; invalidare prove diventate obsolete.
- [x] Conservare i confini: «test dichiarati superati» non equivale a «ogni
  requisito semanticamente dimostrato». Rendere visibili requisiti non coperti,
  verifiche assenti/zero test/interrotte e interventi umani.
- [x] Applicare controlli indipendenti appropriati: unit/integration test,
  proprietà e confini, build/lint/typecheck, artefatti e interazione browser.
  Il modello può proporre test; non può ridefinire il criterio congelato.
- [x] Regressioni con controesempi: check verde su codice sbagliato, checker
  modificato, errore nascosto da exit 0, falsa baseline, test non scoperti.

**Dove:** `crates/pwr-verify/`, `pwr-orchestrator/tests/completion_checks.rs`,
`unrunnable_checks.rs`, stato ACP in `pwr-cli/src/serve.rs` e UI degli esiti.
**Uscita:** nessuna falsa dichiarazione verificata nei casi congelati; copertura
semantica dichiarata e limiti comprensibili. Non promettere un verificatore
universale. Qualsiasi nuova semantica UX viene prima definita con esempi.
**Mappa:** W3, G1/G2 e requisito di zero false verifiche della campagna.

## S03 — Controller, modifiche e contesto

**Chiusura 2026-10-20: DONE con residui, per decisione del proprietario** (serve
una build per i video del post). [Rapporto S03](../reviews/2026-10-20-S03-controller.md):
classificate le cinque tracce del sito; chiuse quattro lacune con regressioni
deterministiche (riga di comando intera in un solo argomento, `operation`
annidata, `delete_path` dopo una riscrittura totale rifiutata, tempo di
verifica/review nel budget). **Non chiusi e non dimostrati:** beneficio sul
modello reale (nessuna inferenza), preflight del prompt effettivo, conservazione
di requisiti e file in compaction/resume, limiti aggregati di turni/goal con
Stop in review, misure prima/dopo. Tornano in S04 (profili e misure per pack) e
S07 (suite dev). Nessun gate promosso.

**Diagnostica CLI 2026-10-19:** [matrice fixing/creazione dei cinque modelli e
candidato E2B thinking](../reviews/2026-10-19-cli-model-usability.md).
Riprodotti errori argv, no-op ripetuti e stalli; corrette perdita delle evidenze
ACP/consegna fra turni e deadline del runner. Restano recupero effettivo,
costo della review e nuove prove sul binario aggiornato. Nessun default promosso.

**Follow-up CLI:** corretto il reset dei rifiuti causato da letture estranee;
replay no-op e controesempio con modifica reale riusciti. Feedback argv mirato
per script Python/Node. Due nuove prove 26B: una fallita per protocollo prima
delle azioni, una creazione completa con goal verificato. Dettagli e limiti
nel rapporto collegato; S03/S04 non chiusi, affidabilità da replicare.

- [ ] Classificare dalle tracce loop, find obsoleti, rewrite rifiutati, no-op,
  errori schema e completamenti ripetuti. Riprodurre prima di cambiare prompt.
- [ ] Recupero deterministico dopo conflitto: nuova lettura/versione e patch
  mirata. Non aggirare una protezione con cancellazione+riscrittura automatica.
- [ ] Provare limiti aggregati di turni ordinari e goal, Stop e tempo speso
  in review. Fermare cicli improduttivi senza rompere fix→test rosso→fix.
- [ ] Preflight del prompt effettivo: obiettivo, policy, tool schema, memoria,
  sorgenti e output. Ridurre/rendere recuperabile l'evidenza prima dell'overflow.
- [ ] Compaction/resume devono conservare requisiti, fatti e riferimenti ai file;
  stato terminale recovery/protocol deve restare un fallimento in tutti i runner.
- [ ] Misurare prima/dopo con stessi pack/task/budget, includendo letture
  legittimamente ripetute, task lungo progressivo e stop anticipati scorretti.

**Dove:** `pwr-orchestrator/src/{converse,context}.rs`, `executor`, `pwr-repo`,
`pwr-runtime/src/window.rs`, runner `evidence/stack-matrix/runner/`.
**Uscita:** ogni modifica ha beneficio misurato o una correzione di contratto,
con controesempi senza regressione; nessun default adattivo promosso per un demo.
**Mappa:** W1.10, W2, W4, W5, W8.3; chiudere la disposizione del percorso scripted.

## S04 — Gemma, compatibilità e prestazioni

- [ ] Controllare template e protocollo nativo per tutti gli otto pack base
  del registro; hash tokenizer/pesi, schema tool, canali, stop, Unicode e cache.
- [ ] Distinguere compatibilità tecnica e affidabilità agentica, per quantizzazione
  e backend. Usare 26B/31B come controlli quando l'hardware li sostiene.
- [ ] Richieste brevi/lunghe, compaction, immagini dove supportate, prefill cancel,
  fault GPU/OOM e ripartenza. Misurare TTFT caldo/freddo, tempo task, memoria,
  idle CPU e responsività dell'app con terminale/browser attivi.
- [ ] Provare assistant/MTP separatamente per parità, stabilità e velocità;
  una accelerazione senza parità non viene abilitata come default.
- [ ] Controllare lock Python, marker/hashes, encoder offline, cache identity,
  prima installazione e aggiornamento; niente nuova risoluzione implicita.

**Dove:** `crates/pwr-mlx/sidecar/`, `crates/pwr-llama/`, `pwr-models`,
`pwr-runtime`, `strategies/`, [engine gestito](../release/managed-engine.md).
**Uscita:** matrice verificabile per pack e limiti hardware; profili supportati
solo con evidenze. Nessun cambio automatico di famiglia: prodotto solo Gemma 4.
**Mappa:** W4/W5, baseline F6, assistenti come decoding e non nuovi agenti.

## S05 — Studiare Harness², WebMCP e strumenti con esperimenti utili

L'obiettivo è rispondere a ipotesi, non installare tutti i framework.
Consultare le [fonti ufficiali e matrice tecnica](#integrazioni-da-studiare).

- [ ] Giorno 1: aggiornare sorgenti/versioni/licenze, definire problema e costo
  dell'integrazione di ciascuna candidata; scegliere al massimo due prototipi.
- [ ] Harness²: correggere l'adattatore privato affinché rifiuti esiti ACP failed,
  max_tokens, scope e budget falliti; verificare l'intero ciclo senza modello.
- [ ] Congelare un nuovo protocollo: contesto editor sufficiente misurato prima,
  baseline e controllo con costo equivalente, repliche, task dev separati e
  arresti immediati. La prova 26B esclusa resta esclusa; non è recuperata retroattivamente.
- [ ] WebMCP: misurare discovery/invocazione su un sito locale posseduto, con
  API disponibile verificata nel browser effettivo. Confrontare con il percorso
  browser attuale sullo stesso flusso, includendo fallback e sito privo di API.
- [ ] MCP/DevTools/ADK: al massimo un test tecnico isolato se serve al problema
  scelto; registrare bridge locali, dipendenze, dati esposti e capacità mancanti.
- [ ] Giorno 5: ADR per ogni candidata con esito ADOPT/DEFER/REJECT/INCONCLUSIVE,
  dati, costi, rischi, requisiti per Windows e lavoro necessario in S06.

**Uscita:** decisioni motivate, non obbligo di adottare. Se il prototipo non è
interpretabile entro la settimana, si chiude il timebox come inconclusivo con
condizioni precise di riapertura. La beta non aspetta ricerca indefinita,
salvi i gate del contratto rimasti vincolanti.
**Mappa:** F2/F4, W8.6, [pilot precedente](../reviews/2026-10-19-harness2-pilot.md).

## S06 — Implementare solo ciò che ha superato la valutazione

- [ ] Scegliere con il proprietario **una** integrazione principale o una
  ottimizzazione PWR misurata. Le altre restano nel backlog con motivazione.
- [ ] Definire un'interfaccia tipizzata, separata dal provider del modello;
  permessi/policy restano nel core. Gestire timeout, cancellazione, errori,
  limiti output e ciclo di vita. Nessuna dipendenza Python/cloud imposta al core
  senza beneficio e decisione esplicita.
- [ ] Per tool esterni: identità/provenienza, allowlist, schema validato,
  revoca su navigazione/chiusura, isolamento workspace/origin, log redatti.
  La descrizione del tool non attribuisce autorizzazioni.
- [ ] Flag sperimentale inizialmente spento, fallback al percorso noto,
  controprove ostili, regressioni e confronto prima/dopo a costo completo.
- [ ] Documentare installazione/disabilitazione e piano di rimozione. Se non
  emerge un vincitore da S05, usare lo sprint per il maggiore costo misurato
  del controller: questo non è uno sprint vuoto.

**Uscita:** beneficio riproducibile entro le soglie prefissate, nessuna
regressione critica e decisione esplicita sul default; oppure modifica ritirata.
**Mappa:** F4/F5, W2/W4/W7. Qui termina l'introduzione di nuove funzionalità 0.5.

## S07 — Qualità coding e preparazione del collaudo

- [ ] Suite dev con fixing, creazione e uso generale definiti sotto; congelare
  fixture, acceptance, budget e numero di repliche prima delle nuove prove.
- [ ] Eseguire sul percorso prodotto ACP e smoke nativo, con controlli fuori
  dal modello. Non usare `eval run` come sostituto senza parità dimostrata.
- [ ] Ripetere le misure sui pack dichiarati per coding e i controesempi su
  quelli leggeri; dichiarare capacità realmente esercitate e supporto per tier.
- [ ] Chiudere G1/G2 e disposizione dei finding F1. Se F6 resta obbligatorio,
  completare endpoint eleggibili, soglie approvate, power analysis e campagna
  prevista: i test manuali sotto non sostituiscono quella prova di superiorità.
- [ ] Eseguire suite automatiche complete, skip inventory, dipendenze/audit,
  installazione frontend pulita e hosted CI sull'identico candidato quando il
  proprietario autorizzerà commit/push necessari. Preflight locale non è CI remoto.

**Uscita:** niente difetti bloccanti, criteri quantitativi concordati soddisfatti,
gate applicabili risolti e protocollo E2E pronto. Un problema sistematico torna
in S02/S03/S04; non viene mascherato aumentando i retry.
**Mappa:** W8, F3/F6, G1/G2/G3 secondo decisione S00.

## S08 — Installazione ad-hoc, distribuzione e aggiornamenti

- [ ] Qualificare il pacchetto macOS ad-hoc attuale, componenti nativi, uv e
  Python/MLX nel bundle. Registrare chiaramente la sua identità di firma e
  l'esperienza di prima apertura. Nessuna iscrizione Apple è richiesta qui;
  Developer ID, notarizzazione e staple passano a S12.
- [ ] Chiudere inventario licenze: Cargo/frontend/font, uv, dipendenze Python,
  distribuzione Python e pesi/modelli; risolvere le 23 review item storiche o
  registrarne la disposizione effettiva. L'inventario parziale non è clearance.
- [ ] Gestire backup indipendente della chiave updater e segreti CI. Configurare
  feed/versioni/canali solo con artefatti reali e dopo autorizzazione prevista.
- [ ] Da un'app precedente installata: Settings→check→download→install→restart;
  firma errata, archivio alterato, rete interrotta, buffer sporchi, attività in
  corso, download annullato e retry. Confermare core/sidecar/versione e dati.
- [ ] Esercitare rollback/reinstallazione assistita senza perdita di dati;
  migrazioni storage compatibili o backup verificato. Non promettere downgrade
  sicuro se lo schema non lo permette.
- [ ] Costruire da sorgenti puliti, registrare provenance/checksum e verificare
  DMG montato più installazione su account/macchina pulita con quarantena reale.

**Dove:** `scripts/release-macos.sh`, `release-provenance.py`, Tauri config,
`updater.rs`, workflow release, [updater](../release/in-app-updates.md),
[distribuzione](../distribution.md). Lo script release pulisce gli output:
eseguirlo solo sul candidato e con il perimetro previsto, mai come test veloce.
**Uscita:** pacchetto ad-hoc installabile e verificato, firma updater e upgrade
nativo provati, supporto/recovery documentati. Il proprietario autorizza di
proseguire con questa qualificazione ad-hoc: non equivale a Developer ID o
notarizzazione e non chiude la successiva qualificazione Apple in S12.

## S09 — Test finale approfondito macOS

- [ ] Congelare commit, bundle installato, pack, manifest e protocollo; verificare
  corrispondenza hash e ripristino dello stato fra casi.
- [ ] Eseguire **tutta** la suite comune seguente dall'app installata: fixing,
  repo da zero, generale, guasti/recupero, installazione/upgrade. Salvare
  trascrizioni, verifiche esterne, screenshot utili e issue con riproduzione.
- [ ] Eseguire su ambiente quotidiano e almeno un utente/host pulito pertinente
  alla matrice; dichiarare ciò che non è disponibile anziché contarlo come pass.
- [ ] Registrare successo autonomo separato da successo con intervento. Una
  correzione crea una nuova identità candidata; ripetere caso e regressioni
  collegate, più smoke globale. Per cambi trasversali ripetere la suite completa.

**Uscita:** tutti i casi obbligatori e le soglie approvate superati, zero bug
bloccanti aperti, limiti residui accettati in modo esplicito. SOLO allora S10.
Il rapporto include anche i primi fallimenti, non soltanto l'ultima esecuzione.

## S10 — Portabilità e qualificazione tecnica Windows

- [ ] Inventariare host Windows/versione, GPU/driver, RAM/VRAM, backend realmente
  implementato, formato modello e quantizzazione. MLX macOS non certifica Windows.
- [ ] Build e CI Windows, installer/prerequisiti WebView2, percorsi con spazi e
  Unicode, case sensitivity, newline, file bloccati e rename/replace atomici.
- [ ] Terminale ConPTY e shell, albero processi/Job Objects ove implementati,
  cancel, Stop/Quit/crash, servizi/porte e impossibilità di lasciare discendenti.
- [ ] Dimostrare isolamento e autorizzazioni realmente disponibili. Se manca
  confinamento equivalente, riportarlo e risolvere il contratto: un'etichetta
  Standard non basta. Nessuna falsa equivalenza con Seatbelt.
- [ ] Gemma sul backend Windows: template, tool-call, cache, stop/prefill,
  memoria/OOM e modelli effettivamente supportati; non assumere parità MLX.
- [ ] Qualificare updater e installer nativi, eventuale firma Windows e relativa
  UX di fiducia. La firma updater non è Authenticode.

**Dove:** `crates/pwr-llama/`, provider/runtime/tools/store, rami `cfg(windows)`
Tauri, workflow e pacchetti. Solo correzioni di piattaforma in questo sprint.
**Uscita:** backend e confinamento dichiarati con prove, installazione pulita,
nessuna perdita dati/processo orfano, CI Windows e regressioni pertinenti verdi.
Modifiche comuni riaprono la verifica macOS prima di procedere.

## S11 — Test manuale end-to-end finale Windows

Ripetere gli stessi ID, specifiche, acceptance e criteri di S09 su workspace
freschi. Cambiano soltanto adattamenti espliciti di ambiente/percorsi/shell,
non i requisiti funzionali. Usare pack Windows qualificati: se backend o
quantizzazione differiscono, il confronto è funzionale per piattaforma,
non un esperimento di superiorità a parità di deployment.

- [ ] Tutti i casi fixing/creazione/generale, crash/resume e aggiornamento.
- [ ] Prove su macchina quotidiana e installazione pulita senza strumenti dev.
- [ ] Triage e nuove identità dopo fix; regressioni comuni riprovate su macOS.
- [ ] Rapporto Windows autonomo con skip, limitazioni, versioni e hash.

**Uscita:** stessa accettazione funzionale di macOS, differenze dichiarate,
nessun bug bloccante. Non contare un caso impossibile da eseguire come pass.

## S12 — Ultimo passaggio Apple, release 0.5 e uso esterno

- [ ] Dopo Windows, quando economicamente disponibile, il proprietario attiva
  l'account Apple. Creare Developer ID, firmare componenti/bundle con
  entitlements appropriati, notarizzare e applicare staple. Nessun acquisto
  viene avviato automaticamente e non è necessario per iniziare gli sprint.
- [ ] Sul nuovo pacchetto firmato ripetere prima installazione con quarantena,
  avvio engine/MLX, permessi, browser/terminale, recupero e aggiornamento da
  versione precedente; la firma può cambiare il comportamento operativo.
  Non trasferire automaticamente gli esiti ad-hoc ai nuovi byte distribuiti.
- [ ] Revisione go/no-go su entrambi i rapporti, gate, licenze, firma, recovery,
  note/changelog, screenshot attuali, matrice modelli e requisiti hardware.
- [ ] Scegliere versione/canale e autorizzare separatamente commit/push/tag e
  pubblicazione. Costruire dal commit validato; verificare hash degli asset
  realmente distribuiti. Nuovi byte di packaging richiedono smoke install/upgrade.
- [ ] Documentazione per utenti: installazione, modelli, primo task, privacy,
  limiti, backup, recupero, aggiornamento, bug report e disinstallazione.
- [ ] Avviare un gruppo di tester esterni con contesti diversi e report
  riproducibili; niente telemetria o invio di repository implicito.
- [ ] Stabilire prima dell'apertura le condizioni per la 1.0: uso ripetuto reale,
  numero/diversità di utenti concordati, finestra di stabilità, bug critici
  chiusi, upgrade affidabili e supporto sostenibile. Non basta il download count.

**Uscita:** release esplicitamente approvata e verificata, oppure no-go motivato.
La 1.0 rimane una decisione successiva fondata su feedback; nessun automatismo
alla scadenza della settimana. Se l'account Apple non è ancora disponibile,
la parte firmata resta BLOCKED senza bloccare gli sprint precedenti. Il
proprietario deciderà separatamente se attendere o distribuire una beta ad-hoc
con limiti dichiarati: questa guida non autorizza automaticamente tale release.

## Suite comune di collaudo finale

È la base proposta da dettagliare e approvare in S00/S07, non test già eseguiti
né soglie scientifiche F6. Definire istruzioni, fixture iniziale, risultati
attesi e budget per ciascun caso; verificatori fuori dalla cartella editabile.
I test dev già visti non diventano task futuri/heldout. I 40 riservati restano
fuori salvo la loro autorizzazione prevista.

| Gruppo / ID | Casi minimi proposti | Verifica indipendente |
|---|---|---|
| Fixing FX01–FX06 | Bug di logica/Unicode, transazione atomica, errore async, regressione multi-file, problema build/dependency, refactoring con compatibilità | Test baseline rossi pertinenti, bug risolto, suite preesistente senza regressioni, scope/diff corretto, nessun test protetto alterato |
| Creazione CR01–CR06 | CLI Python, CLI/libreria Rust, web app TypeScript, API con persistenza, frontend+backend, evoluzione di un progetto appena creato | Partenza vuota verificata, struttura/dependency lock/README, install/build/run puliti, unit/integration test e uso reale; UI controllata anche nel browser |
| Uso generale UX01–UX12 | Onboarding, modello download/cancel, selezione e cambio workspace, conversazione/goal, editor/diff/revert, terminale, browser, stop, resume, conflitto esterno, offline, errori/retry | Stato UI coerente con core, file finali, richieste di permesso, stato persistito, responsività e nessun effetto inatteso |
| Recovery RC01–RC06 | Crash core, crash app, record corrotto, disco/rete insufficienti, engine setup interrotto, update/restart/recovery | Dati conservati o errore onesto, nessun falso ready/completed, processi/porte chiusi, recupero documentato |
| Distribuzione DS01–DS04 | Prima installazione pulita, upgrade precedente→candidato, firma/archive negativo, reinstallazione/rollback supportato | Bundle/core/sidecar/versioni corretti, integrità/firma, stato utente preservato, canale corretto |

Sono **34 scenari proposti** (6+6+12+6+4), distinti dai 40 task riservati del
benchmark: questa suite non autorizza l'uso di quel corpus. Le tre
stack e la complessità dei sei progetti sono da concordare con il target reale;
non introdurre stack non supportati per gonfiare la matrice.

Eseguire tutti gli scenari obbligatori sul profilo di riferimento e una matrice
ridotta ma prefissata per gli altri pack supportati. Questa riduzione riguarda
il collaudo prodotto e non sostituisce l'eventuale campagna F6 per tier.
Per affidabilità registrare più esecuzioni nei casi stocastici secondo il
protocollo approvato. Latenza/memoria hanno soglie misurate su host dichiarati;
non inventare un unico limite universale di 150 secondi.

Classi bug: B0 sicurezza/perdita dati/falsa verifica critica; B1 flusso essenziale
rotto; B2 degradazione con workaround accettabile; B3 presentazione. B0/B1
bloccano avanzamento e release. Ogni B2 residuo richiede impatto, workaround e
accettazione scritta; non può essere una perdita dati o bypass nascosto.

## Integrazioni da studiare

Fonti controllate il **2026-10-19**. Lo stato upstream va ricontrollato all'inizio
di S05. Le valutazioni «priorità/costo» sotto sono scelte proposte per PWR,
non risultati sperimentali o promesse degli autori.

| Tecnologia | Cosa offre / stato verificato | Prova proposta e decisione per PWR |
|---|---|---|
| Gemma 4 function calling | Guida ufficiale su schema tool e ciclo modello→esecutore→risposta | Priorità alta: parità template/schema/canali nel nostro MLX/llama. Gli esempi Transformers non qualificano automaticamente un runtime diverso |
| Harness² | Ricorsione sul task e lezioni tra task; pilot PWR inconclusivo | Ricerca circoscritta: adattatore rigoroso, contesto sufficiente, C vs B/D e costo completo. Prima istruzioni dichiarative, poi eventuali lezioni cronologiche isolate |
| WebMCP | Proposta di standard web per tool strutturati; documentazione Chrome aggiornata al 7 ottobre indica origin trial da Chrome 149 | Probe browser dedicato: disponibilità reale, sito locale, strumenti dichiarativi/imperativi, fallback. Nessuna assunzione di supporto nel webview Tauri macOS |
| MCP e Chrome DevTools MCP | Protocollo per tool esterni; progetto Chrome per controllo/debug del browser da agenti | Valutare prima un'interfaccia isolata per test web/diagnostica, con profilo browser dedicato; evitare duplicazioni e privilegi sul browser personale |
| Google ADK | Framework con modelli/connettori e integrazione MCP; documentazione Gemma include Gemma 4 | Utile come riferimento o concorrente controllato. Misurare il costo di un adapter al deployment locale; nessuna riscrittura del core Rust per affinità di marchio |
| Altri framework / A2A / nuovi modelli specializzati | Candidati da censire soltanto in presenza di un problema concreto | Per ora backlog: non allargare famiglie, servizi cloud o delega solo per completare la lista. Nuove feature/default restano decisioni del proprietario |

WebMCP e un server MCP esterno sono superfici diverse: nel probe WebMCP il sito
espone azioni nel browser visitato; nel probe MCP si collega un servizio
esterno. Non promettere che installare un server abiliti WebMCP nel webview.
Per PWR la compatibilità con WKWebView/WebView2 è **da misurare**. Un bridge
Chrome potrebbe richiedere un componente aggiuntivo: è un costo/decisione,
non una funzione già presente.

Controlli del probe WebMCP: origine/frame e tool ID, schema/input/output,
registrazione/revoca dopo navigazione, azioni sensibili con conferma,
timeout/cancel, pagina ostile e istruzioni nel contenuto. Nessun bridge deve
concedere a una pagina i comandi IPC dell'app. La fixture verifica separatamente
l'azione riuscita, il rifiuto e il fallback su siti senza tool strutturati.
Sono requisiti progettuali PWR, non garanzie della specifica web.

**Regola d'adozione:** prima del probe fissare beneficio minimo e costo massimo
rispetto alla baseline, compresa l'incertezza; zero nuove violazioni critiche.
Esempi da quantificare con la baseline: più task corretti, meno interventi,
minor tempo/costo con qualità non inferiore. La soglia non si sceglie dopo il
risultato. Una tecnologia può migliorare il browser senza migliorare il fixing:
non trasferire il claim a un altro dominio.

Fonti primarie:

- [Gemma 4 function calling](https://ai.google.dev/gemma/docs/capabilities/text/function-calling-gemma4).
- [Harness² repository](https://github.com/google-research/harness2) e [guida esperimenti](https://github.com/google-research/harness2/blob/main/docs/EXPERIMENTS.md).
- [WebMCP: documentazione corrente Chrome](https://developer.chrome.com/docs/ai/webmcp). Il vecchio annuncio di febbraio non descrive da solo lo stato di ottobre.
- [Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp).
- [ADK: Gemma](https://adk.dev/agents/models/google-gemma/) e [ADK: MCP](https://adk.dev/tools-custom/mcp-tools/).
- [MCP security guidance](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices).
- [Apple Developer ID](https://developer.apple.com/developer-id/) e [iscrizione Apple](https://developer.apple.com/programs/enroll/): 99 USD/anno indicati da Apple, prezzo locale e condizioni da verificare nell'iscrizione. Non è un acquisto eseguito da questa guida.

## Aggiornare la guida ogni settimana

1. All'apertura scegliere sprint e ticket, baseline/hash, criteri e dipendenze.
2. A ogni fix associare riproduzione, regressione ed esito; IMPLEMENTED senza
   evidenza non è DONE. Salvare dati privati fuori dai documenti pubblici.
3. Alla chiusura compilare un [rapporto sprint](sprint-report-template.md),
   aggiornare la tabella qui e collegare risultati/issue/limitazioni. Nel corpo
   pubblico usare ID evidenza o riferimenti pubblicabili, non percorsi personali.
4. Aggiornare milestone/gate soltanto con l'evidenza richiesta nel manifest;
   per modifiche di contratto aggiungere una decisione motivata, senza riscrivere
   il passato. Aggiornare readiness e rigenerare roadmap con lo script esistente.
5. Ogni bug del collaudo torna al relativo sprint. Un nuovo candidato invalida
   solo le prove che dipendono dalla modifica, con motivazione; per cambi
   trasversali serve nuova verifica completa. Nessun «tutto passato» per somma
   di risultati appartenenti a candidati diversi.

Stati consentiti: PLANNED, IN_PROGRESS, BLOCKED (con dipendenza), DONE (con
rapporto ed evidenze), DEFERRED (con decisione e impatto). Prossimo sprint:
**S02**, dopo la chiusura tecnica S01 documentata il 2026-10-20.
