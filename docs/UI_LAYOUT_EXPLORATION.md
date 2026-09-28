# PWR — esplorazione dell'interfaccia agentica

Base: `design/themes` al commit `2ad41e9e`. Le sei proposte qui sotto si aggiungono a Studio, Instrument, Paper, Islands, Mission e Focus. Sono esperimenti frontend: lo stesso `AgentStore`, `RunStore`, conversazione, composer, workbench, modelli e dialoghi restano attivi. Aprire `?demo&variant=relay` nel browser per un task registrato; dentro PWR usare il selettore «Layout» nella barra o Settings → Appearance. I colori dark e light si scelgono indipendentemente dal layout.

## Analisi dello stato iniziale

| Area | Stato osservato | Opportunità |
| --- | --- | --- |
| Struttura | Tauri + Angular con shell intercambiabili, store condivisi e ACP verso il core. Studio usa sidebar, conversazione e workbench affiancati. | La modularità consente esperimenti senza toccare esecuzione o protocollo. |
| Navigazione/sessioni | Sidebar con modalità Chat/Agent, workspace e cronologia; gli shell senza sidebar usano il session switcher. Scorciatoie e command palette sono già presenti. | Nei layout focalizzati sul task la cronologia può scomparire dalla vista: mantenere il cambio sessione sempre raggiungibile. |
| Conversazione | Risposte in streaming, fasi raggruppate, reasoning comprimibile, tool e risultati nel turno; composer con allegati, queue, Goal, permessi e stop. | Il messaggio resta spesso il contenitore sia della risposta sia della telemetria: pesa sulla scansione durante run lunghi. |
| Attività agente | `RunStore` ricava fasi, azioni, stato e risultato dalla timeline. Strip, margine e board visualizzano gli stessi eventi. | Il processo è comprensibile, ma plan/inspect/edit/check non costituiscono ancora una navigazione unica fra evento, file e verifica. |
| Tool/output | Workbench con Review, Plan & checks, Activity, Files, Terminal, Browser, Knowledge; card comprimibili, riordinabili e ingrandibili. | Molte card competono in una colonna stretta; un output importante può rimanere nascosto sotto altre card. |
| File/diff | Review mostra file modificati, linee aggiunte/rimosse, diff e revert via core. | Il legame fra azione nella timeline e diff pertinente è visibile sulla board solo per gli edit; il focus automatico sul file attivo resta da esplorare. |
| Context/token/model | Model picker, context meter con dettaglio, velocità e compatibilità hardware. Model Manager copre macchina, engine, ricerca, fit e download. | Il context nella topbar può competere con il task; il costo in token ha più valore come pressione o dettaglio su richiesta che come numero persistente ovunque. |
| Stato/verifiche | Stato run, permessi e sandbox; fase Verify e check derivano dagli eventi reali. | Una fase «Verifying» o un conteggio di check non equivale a una prova complessiva di successo. La UI deve mostrare l'esito dei singoli tool senza inventare un badge globale «verified». |
| Impostazioni/temi | Aspetto System/Light/Dark, palette dark e light separate, sei layout iniziali e token semantici. | È possibile combinare filosofia di layout e linguaggio visivo; le nuove direzioni sfruttano i temi già disponibili. |
| Responsive/densità | Studio sgancia prima l'inspector e poi la sidebar; altre shell hanno drawer, rail o dock. | Il collasso protegge la conversazione, ma può rendere invisibile l'attività proprio quando l'agente opera. Servono priorità diverse per stato e compito. |

**Elementi riusciti.** Store e componenti sono riutilizzabili; la telemetria è derivata da eventi del core; Review, permessi, sandbox e modello rendono PWR più controllabile di una chat generica. La separazione layout/palette del branch `design/themes` è una base forte per il confronto.

**Ridondanze e gerarchia.** Context, velocità e modello possono comparire sia in alto sia nella status bar. La stessa sequenza di fasi può apparire nella conversazione e in strip/board. Lo spazio verticale delle card e il padding della conversazione si sentono soprattutto nelle finestre medie. La status bar è utile, ma nelle viste sperimentali la presenza contemporanea della barra alta è deliberatamente da confrontare.

**Limiti dei dati.** Il frontend espone stato, output, diff, usage, context e modello. Non espone una provenienza completa dei segmenti di context, né un giudizio verificato dell'intero task. Non sono stati aggiunti valori sintetici per riempire queste lacune. Il browser `?demo` usa dati registrati: l'app Tauri usa gli eventi reali del core.

## Sei nuove direzioni

### 1. Relay — sequenza prima della chat · conservativa

- **Concetto e struttura:** la fase corrente e quelle raggiunte costituiscono una spina verticale a sinistra; conversazione al centro, workbench a destra.
- **Gerarchia e navigazione:** run → contenuto → evidenza. Session switcher sempre in alto; fase leggibile anche mentre si scorre la chat.
- **Chat, reasoning e tool:** la chat conserva reasoning comprimibile e dettagli dei tool; la spina riassume le fasi e il workbench apre output, diff e file.
- **File, context, modello:** Review è a un passaggio; context e modello in alto e nella barra di stato, con dettagli nei popover.
- **Vantaggio:** chi arriva da Studio trova rapidamente il filo causale di un task. **Svantaggio:** tre colonne richiedono larghezza; su mobile la spina è nascosta e resta la conversazione con la zona strumenti.
- **Funziona bene per:** run lunghi in cui si vuole capire dove si trova l'agente senza abbandonare il dialogo. **Linguaggio visivo:** high-density engineering; Code Dark/Light o High Contrast.

### 2. Workshop — evidenza prima della chat · conservativa

- **Concetto e struttura:** il workbench è la superficie principale; una colonna di task riassume le azioni, la conversazione è un sidecar compatto; le fasi attraversano la parte alta.
- **Gerarchia e navigazione:** file/diff/test → task → dialogo. Il workbench mantiene le card reali, perciò terminal, Review e Files restano utilizzabili.
- **Chat, reasoning e tool:** composer e risposta vivono nel sidecar, il reasoning si espande lì; gli output dei tool ricevono larghezza centrale.
- **File, context, modello:** diff e file hanno la massima area; context e modello restano nei controlli della cornice.
- **Vantaggio:** rende naturale verificare il lavoro. **Svantaggio:** leggere risposte lunghe nel sidecar è faticoso; il board stretto mostra meno dettagli.
- **Funziona bene per:** coding e revisione di modifiche. **Linguaggio visivo:** modern IDE; Code Dark/Light o Arctic.

### 3. Chronicle — registro operativo · innovativa

- **Concetto e struttura:** la conversazione diventa un registro ampio, con fase corrente sopra e un vassoio orizzontale di evidenze sotto.
- **Gerarchia e navigazione:** tempo → contenuto → prova. L'utente segue un unico flusso e apre i dettagli dove servono.
- **Chat, reasoning e tool:** messaggi e riassunti dei tool restano nella timeline reale; reasoning comprimibile, card di output affiancate nel vassoio.
- **File, context, modello:** Review e terminal possono occupare card orizzontali; context e modello rimangono consultabili dalla cornice.
- **Vantaggio:** ottima continuità narrativa senza sidebar. **Svantaggio:** il vassoio riduce l'altezza della conversazione e molte card richiedono scorrimento orizzontale.
- **Funziona bene per:** audit, replay di sessioni e spiegazione del lavoro. **Linguaggio visivo:** editorial tecnico; Linen/Solar o Midnight.

### 4. Pulse — gerarchia dipendente dallo stato · innovativa

- **Concetto e struttura:** a riposo prevale la conversazione; durante l'esecuzione cresce la board; in Verifying/Fixing cresce il workbench. La geometria usa lo stato già derivato da `RunStore`.
- **Gerarchia e navigazione:** lo spazio segue la fase, mentre status, sessione e composer restano coerenti.
- **Chat, reasoning e tool:** la chat rimane sempre raggiungibile; i tool passano in primo piano quando è utile controllare l'esito. Reasoning e output conservano i disclosure esistenti.
- **File, context, modello:** durante la verifica Review/Plan/Terminal ricevono più altezza; context e modello restano fissi nella cornice.
- **Vantaggio:** rende visibile la transizione thinking → acting → checking. **Svantaggio:** il movimento dei pannelli può disturbare chi cerca una posizione stabile.
- **Funziona bene per:** seguire un agente live. **Linguaggio visivo:** technical dashboard sobrio; Arctic o Code Dark/Light.

### 5. Map — workspace spaziale · sperimentale

- **Concetto e struttura:** la board dei task è un campo ampio; la conversazione corre sul lato, gli strumenti formano una base orizzontale.
- **Gerarchia e navigazione:** fase e azione → dialogo → prova. Le card della board mostrano tool, esito e diff quando disponibili.
- **Chat, reasoning e tool:** messaggi e reasoning restano nel sidecar; la board organizza l'attività in colonne, il vassoio strumenti espone i dettagli.
- **File, context, modello:** gli edit nella board aprono Review; context e modello sono parte della cornice, non nodi fittizi del canvas.
- **Vantaggio:** rende scansionabile il flusso di azioni senza ridurlo a una sequenza di messaggi. **Svantaggio:** una board larga perde utilità in finestre piccole e sul mobile viene privilegiato il dialogo.
- **Funziona bene per:** task con molti step e interventi su diversi file. **Linguaggio visivo:** modular grid; Ember o Midnight.

### 6. Deck — una superficie per volta · sperimentale

- **Concetto e struttura:** Task, Dialogue e Tools sono tre viste a larghezza piena, raggiungibili da tab espliciti. Nessuna sidebar o inspector permanente.
- **Gerarchia e navigazione:** l'utente sceglie il contesto cognitivo attivo; Task mostra la board, Dialogue mostra la conversazione e il composer, Tools il workbench.
- **Chat, reasoning e tool:** reasoning rimane nel dialogo, mentre output e file possono occupare tutta la vista Tools. Il task mantiene l'attività reale, compresi gli esiti disponibili.
- **File, context, modello:** la vista Tools lascia spazio a diff, terminal e Files; la cornice persistente mostra context e modello.
- **Vantaggio:** riduce il rumore e funziona anche in finestre strette. **Svantaggio:** il confronto simultaneo fra codice, output e dialogo richiede passaggi di vista.
- **Funziona bene per:** focus profondo, laptop piccoli e ispezione di un singolo artefatto. **Linguaggio visivo:** calm minimal; Linen o Code Light.

## Cosa combinare in seguito

- **Relay + Workshop:** mantenere una traccia di fase persistente mentre Review diventa la superficie centrale durante l'editing.
- **Pulse + Deck:** passare automaticamente alla vista pertinente solo quando l'utente lo abilita; mantenere le tab come controllo esplicito.
- **Chronicle + Map:** selezionare un'azione nel registro e mettere in evidenza la card corrispondente nella board e il suo diff.
- **Ogni layout:** distinguere sempre «fase raggiunta», «tool completato» e «risultato verificato». I dati attuali sostengono i primi due, non un giudizio globale del terzo.

Queste direzioni sono intenzionalmente confrontabili e nessuna viene proposta come vincitrice.
