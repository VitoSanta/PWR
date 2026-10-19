# Harness² su PWR — pilot di fattibilità, 18–19 ottobre 2026

**Esito: nessun beneficio dimostrato; efficacia inconclusiva. Nessuna adozione nel prodotto.**
La demo ufficiale funziona. Il primo adattatore PWR attraversa il ciclo upstream,
ma alcune sessioni editor falliscono per contesto/protocollo e vengono trattate
erroneamente come una guida invariata valida. I controlli indipendenti trovano
inoltre errori di correttezza non coperti dai test visibili. La prova 26B viene
esclusa dal confronto per modifiche concorrenti ai sorgenti.

Il proprietario ha scelto di completare questo rapporto e rinviare una nuova
prova 26B a dopo i lavori UI. Nessuna inferenza resta attiva a fine campagna.

## Metodo e ambito effettivamente provati

Sorgente ufficiale: [google-research/harness2](https://github.com/google-research/harness2),
commit `1cfe254e8d25b15f37048c3730134686abe60335`, conservato anche come archivio.
La demo upstream con `k=1` completa due task senza errori, usando entrambe le
modalità e l'archivio di lezioni deterministico: 14 esecuzioni Python e 16
chiamate editor scriptate. Questo non misura Gemma o la qualità di PWR.

Il pilot con modelli usa **il codice upstream non modificato**
`run_parallel(k=1)` con un adattatore privato PWR/ACP. La superficie di prompt
usa il profilo upstream chiamato `opencode`, necessario al suo generatore;
l'esecutore resta PWR, non OpenCode. Solo due file Markdown possono cambiare,
per un massimo complessivo di 6.000 caratteri. Niente script, plugin,
subagent, modifica di policy/guardrail o promozione di default.

Sono congelati due task sintetici di sviluppo, creati appositamente per
requisiti di parsing e atomicità già problematici; non sono task nuovi di
validazione generale. Non vengono usati heldout, i 40 riservati o benchmark
Google. Questo è uno stadio ridotto di fattibilità, non la campagna proposta
nel [piano](../plan/piano-campagne-gemma4-e-windows.md#3-controller-e-ricerca-per-i-modelli-piccoli-incl-harness²)
con sei task dev, quattro futuri, repliche e successiva conferma.

Configurazione: Gemma 4 E2B/E4B/26B 4-bit installati, thinking e assistant
spenti, contesto richiesto 16.384 token, sampling PWR invariato, nessun nudge.
Ogni prompt ha un limite di 150 secondi; setup/chiusura sono misurati a parte
nel tempo della sessione. Un solo motore locale, nessuna compilazione, test o
modifica PWR da questa campagna durante l'inferenza. Sandbox standard richiesta;
le tre richieste di autorizzazione osservate con E4B sono tutte rifiutate.
I checker indipendenti girano dopo l'inferenza, in copie temporanee con rete e
scritture esterne negate. Hash completi di modello, runtime, fixture e driver
sono registrati prima delle prove.

## Confronto e selezione

- **B:** una esecuzione della guida di base, condivisa con C e D.
- **C:** B, due editor (generale/specifico), due probe, compositore ed esecuzione
  finale su fixture pulita: sette chiamate, includendo il costo degli editor.
- **D:** B e sei ulteriori esecuzioni della guida invariata. Se un editor C
  richiede un retry, D riceve una chiamata aggiuntiva. Si seleziona il primo
  artefatto entro budget che passa i test visibili e il controllo dello scope;
  l'audit indipendente non entra nella selezione.

Sono uguali numero massimo di chiamate e limite per prompt, **non** i token
consumati o il tempo effettivo. Il controllo D ha anche una selezione
conservativamente favorevole tramite test visibili. C precede D, una sola
replica per cella: ordine, cache e carico non sono controbilanciati. Nessuna
ricostruzione storica del braccio A viene tentata; non si attribuiscono guadagni
ai precedenti fix deterministici. Nessuna memoria di lezioni viene attivata nel
pilot PWR; quella della demo scriptata non è un risultato con Gemma.

Successo prefissato: test visibili **e** audit indipendente congelato superati,
scope rispettato, nessun annullamento per budget. L'audit comprende Unicode,
precisione intera, bool, convalida dell'intero dizionario e stato invariato su
rifiuto. I checker accettano implementazioni di riferimento prima delle prove.

## Risultati

| Pack e task | B | C, Harness² | D, guida invariata con più tentativi | Interpretazione |
|---|---|---|---|---|
| E2B, parser importi | Non supera i controlli | Non supera i controlli | Non supera i controlli | Editor generale/compositore esauriscono il contesto; guida finale invariata |
| E4B, parser importi | Test visibili verdi, audit rosso; fuori tempo | Test visibili verdi, audit rosso | Test visibili verdi, audit rosso | Modifiche testuali valide, nessun successo completo |
| E4B, trasferimento atomico | Test visibili e audit rossi; fuori tempo | Test visibili verdi, audit rosso | Test visibili verdi, audit rosso | Il compositore fallisce per protocollo; guida finale invariata |
| 26B, parser importi | Escluso | Escluso | Escluso | Guard sorgenti fallito: modifiche UI concorrenti; nessun risultato comparativo valido |

Nei tre confronti con artefatti finali valutabili e sorgenti rimasti invariati,
nessun braccio produce un successo completo secondo l'endpoint prefissato.
Questi dati **non stimano** un tasso generale né dimostrano che il metodo
Harness² sia inefficace: due composizioni non sono terminate correttamente.

E2B sul trasferimento resta parziale: il compositore crea
`context/harness/transfer.py`, fuori dai due Markdown consentiti. La proposta
non viene applicata; l'operatore ferma il controller al riscontro della
violazione. Nessun D completo o esito finale viene attribuito a questa cella.

### Errori concreti e dichiarazioni di completamento

Gli artefatti finali E4B passano i test dichiarati e il core restituisce
`verified=true`, ma l'audit trova violazioni del contratto:

- Nel parser, conversioni attraverso float perdono precisione sugli importi
  interi grandi; restano anche input non validi accettati.
- Nel trasferimento, alcuni valori bool non sono rifiutati come richiesto,
  inclusi saldi preesistenti. I test visibili non coprono tutti questi confini.

Si preservano entrambi gli esiti: verifica dichiarata riuscita e specifica
completa non rispettata. Non si cambia il checker dopo aver osservato il codice.
La revisione del modello non sostituisce una verifica indipendente adeguata.

### Costi osservati dei confronti non contaminati

| Pack e task | Chiamate C / D | Minuti C / D | Token prompt riportati C / D | Token generati C / D |
|---|---:|---:|---:|---:|
| E2B, parser | 7 / 7 | 7,05 / 9,86 | 1.163.072 / 1.677.838 | 26.231 / 41.061 |
| E4B, parser | 7 / 7 | 13,58 / 16,02 | 483.983 / 370.596 | 29.009 / 32.865 |
| E4B, trasferimento | 8 / 8 | 13,83 / 17,87 | 535.409 / 459.557 | 29.507 / 29.753 |

I tempi sono la somma delle sessioni, inclusi setup/chiusura. La baseline
condivisa è inclusa in ciascun braccio e va sottratta se si somma il costo
complessivo del caso. I token prompt sommano le generazioni, incluse le parti
ripresentate dalla cache; non sono token unici. Nessuna misura energetica o
costo monetario è stabilita. I valori USD zero del ledger upstream sono campi
non valorizzati dall'adattatore locale, non prova di costo nullo.

## Arresti e limiti dell'adattatore

1. **Prima infrastruttura:** il nome di superficie inventato dall'adattatore non
   è accettato dal generatore upstream. Il primo baseline E2B resta conservato;
   la revisione v2 corregge solo il nome, dopo una verifica senza modello
   dell'intero ciclo con quattro rollout e tre chiamate editor.
2. **Scope E2B:** arresto conservato separatamente, nessuna proposta Python
   applicata. Il primo driver controlla lo scope prima di raccogliere la guida,
   ma propaga l'arresto della campagna troppo tardi. Le esecuzioni separate v3
   degli altri pack propagano immediatamente la violazione fuori dal fallback.
3. **Esiti editor mascherati:** il primo adattatore verifica errore di trasporto,
   budget e scope ma non l'esito ACP strutturato. `terminal.status=failed`, con
   classe recovery/protocol, può quindi arrivare a upstream come guida invariata.
   `mode.ok=true` significa ciclo schedulato, non editor riuscito o task corretto.
   Un controllo successivo offline rifiuta questi stati e passa cinque
   regressioni. Non viene applicato retroattivamente ai driver congelati e non
   vengono ricalcolati i risultati originali fingendo un'esecuzione diversa.
4. **26B:** il guard registra cambiamenti a Model Manager, picker, icone e CSS
   durante il parser. Binario e sidecar privati restano identici, ma il protocollo
   vieta comunque modifiche PWR durante inferenza: la cella è esclusa. Il secondo
   task non parte. Nessuna qualificazione viene recuperata a posteriori.
5. **Riavvio host:** la prova E4B era già completa. Il checkout upstream temporaneo
   viene recuperato dall'archivio conservato prima di avviare 26B. I log ACP/MLX
   permanenti restano disponibili; il precedente stdout temporaneo non sopravvive.

Il raw `results.json` v2 conserva `running` perché l'interruzione operatore
esce come KeyboardInterrupt; `protocol-stop.json` registra l'arresto effettivo.
La prima sonda diagnostica dopo l'arresto 26B ha riconosciuto come motore il
proprio comando shell contenente la stringa cercata: l'inventario finale della
campagna e la sonda isolata successiva sono vuoti. Non era un motore residuo.

## Evidenze e prossima decisione

Evidenze private distinte, senza sovrascritture:
`harness2-pilot-20261018`, `harness2-pilot-20261018-v2`,
`harness2-pilot-20261018-v3-e4b`, `harness2-pilot-20261018-v3-26b`.
Contengono protocollo/driver e relativi hash, archivio upstream, snapshot del
runtime, checksum modelli, prompt/tracce, workspace finali, esiti indipendenti,
costi e motivi di arresto. Sono osservate 62 sessioni con modello complessive,
includendo preparazione fallita, celle parziali ed escluse; non sono 62 task
indipendenti né 62 successi/fallimenti comparabili.

Prima di una nuova prova: terminare i lavori UI, riconciliare l'identità del
candidato e congelare un protocollo nuovo. L'adattatore deve rifiutare gli
errori strutturati degli editor e verificare preventivamente che il contesto
contenga l'evidenza necessaria; non basta aumentare un limite dopo aver visto
un risultato negativo. Contesto/budget più ampi richiedono confronti e costi
nuovamente prefissati. Solo dopo una prova interpretabile ha senso estendere
il campione o introdurre lezioni riutilizzabili. Nessuna feature, sampler,
permesso, release o gate è promosso da questi risultati. Windows resta ultimo.
