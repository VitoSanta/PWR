# Stato del prodotto — 1 ottobre 2026

**Scritto sul codice di `develop` a `e2e0bbfb`, dopo una giornata di prove
dirette sull'app reale.** Risponde a quattro domande: dove siamo, che cosa PWR
riesce a fare davvero, quali sono i limiti noti, che cosa manca per rispettare
il principio di prodotto ([MASTER_SPEC](../../MASTER_SPEC.md)). Dove un numero
è citato, ha le sue condizioni accanto; dove non c'è una misura, lo dico.

## In una frase

PWR oggi è **un agente di codice locale che funziona, con modelli da 20–35
miliardi di parametri, su compiti piccoli e medi con test chiari; non è ancora
dimostrato che renda meglio di un ciclo semplice, e non ha ancora superato i
tre cancelli (G1, G2, G3) che il piano ha posto prima di poterlo affermare.**
La parte "locale, controllabile, con gli effetti dichiarati" è reale e
verificata. La parte "l'harness migliora il modello" resta un'ipotesi.

## Che cosa fa davvero (verificato)

- **Gira tutto sul Mac**: app Tauri + Angular, un processo Rust (`pwr serve`)
  e un motore MLX proprio. Nessun servizio esterno per generare.
- **Lavora su una cartella reale**: legge, cerca, modifica, esegue comandi in
  una sandbox (Seatbelt), chiede il permesso per ciò che esce dai limiti,
  mostra i diff, tiene un registro degli eventi con catena di hash.
- **Verifica dall'esterno**: dopo le modifiche lancia i controlli del
  repository e dice con onestà il risultato, compreso "non verificato" quando
  non c'è un controllo di accettazione dichiarato. Il modello non dichiara mai
  da solo un fatto verificato.
- **Compatibilità dei modelli**: legge i formati di chiamata di otto famiglie
  (Qwen, GLM, Seed, Gemma 4, gpt-oss, Granite, Mistral/Devstral, Liquid/LFM2) e
  calibra ogni modello prima di usarlo. **Tutti e 22 i modelli scaricati
  sull'Mac oggi superano i controlli critici di calibrazione.** Stamattina non
  era così: molti risultavano "Limited" per difetti nostri, non dei modelli.
- **Misura sul percorso reale**: il banco di prova (`evidence/stack-matrix`)
  pilota `pwr serve` come fa l'app, con una "persona" simulata che concede solo
  ciò che il compito nomina, e verifica il risultato **fuori da PWR**, in
  Docker, con test nascosti. Ogni compito è provato solubile (il seme fallisce,
  la soluzione di riferimento passa).

## Che cosa ho misurato, e con quali limiti

| Misura | Esito | Condizioni |
|---|---|---|
| Qwen3.6-35B-A3B, 8 compiti dev, **prima** del review esteso | 3/8 | una prova per compito |
| stesso modello e compiti, **dopo** il review esteso | 6/8 | una prova per compito |
| Qwen3.5-9B su `bash-rotate` | fallisce, con e senza il campionamento corretto | cicli di ragionamento |
| Qwen3-Coder-30B su `bash-rotate` | fallisce in tutte e tre le prove con memoria sotto controllo (la serale: 130 azioni in 60 min) | modello in loop di riscritture; le prove con tre modelli insieme sono escluse |
| gli altri modelli (Devstral, Gemma 26B, Ornith 35B, GLM, Nemotron, gpt-oss) | **non ancora misurati** sul codice | campagna fermata |
| compiti `heldout` | **mai eseguiti** | l'harness non era congelato |

**Lettura onesta**: 3/8 → 6/8 è un segnale incoraggiante, ma con una prova per
compito e un output stocastico non è una prova. Nessun risultato qui sostiene
che l'harness migliori il modello rispetto a un ciclo semplice: questa è
esattamente la cosa che il contratto di prodotto chiede di dimostrare, e non è
stata ancora misurata. Le prove fatte con più modelli in parallelo sono state
scartate, perché tre modelli insieme (circa 45 GB) saturavano la memoria GPU e
producevano testo senza senso. I tempi delle prove con due corsie parallele non
valgono come velocità.

## Limiti noti

**Dei modelli.**
- Sui compiti difficili, i modelli da 9–30 miliardi **girano in tondo**:
  riscrivono lo stesso file decine di volte con gli stessi test che falliscono
  (Qwen3-Coder: circa settanta riscritture di `bin/rotate`), oppure collassano
  in testo degenerato dopo molti tentativi nella stessa conversazione (a circa
  27.000 token). PWR oggi li rileva e interviene (cancellazione e ripartenza
  della conversazione, nota ogni dodici riscritture), ma **l'effetto di questi
  due interventi non è ancora misurato**.
- Il pensiero lungo dei modelli "reasoning" rallenta molto: GLM-4.7-Flash ha
  fatto 11 azioni in oltre un'ora (con due modelli in parallelo, quindi il
  numero è pessimistico, ma l'ordine di grandezza è quello che hai visto).
- I modelli piccoli sbagliano **regole esplicite del testo** (righe vuote da
  ignorare, comportamento di `--keep 1`): il review esteso aiuta, non basta.

**Del motore e della memoria.**
- **Un solo motore alla volta**: due modelli caricati insieme non stanno nella
  memoria GPU utile (circa 48 GB su un Mac da 64 GB) e il testo si degrada.
- **Cambiare modello a metà conversazione** costringe a rileggere tutto il
  prompt da zero: minuti, a volte decine di minuti. Oggi l'app mostra "Sto
  leggendo la conversazione 37%", ma non avvisa prima del cambio.
- Se una risposta interrotta non si scarica in 300 secondi, il motore viene
  riavviato e rilegge tutto (3–4 minuti a 40.000 token).
- **Nemotron e GLM** nelle conversazioni lunghe: la causa del degrado non è
  nota.

**Della verifica.**
- Senza un controllo di accettazione dichiarato, un lavoro non è mai
  "verificato" (lo dice, ma resta una fiducia limitata). Il review esteso
  legge il testo del compito contro il codice, ma è lo stesso modello a farlo.
- Solo `.pwr/checks.json` è congelato; i test e gli script che un controllo
  esegue non lo sono ancora del tutto.
- Compiti che richiedono **installare un toolchain** (Go assente sulla
  macchina) dipendono da come il modello scarica: un modello che tenta
  `curl` fuori dalla sandbox viene rifiutato e il compito fallisce.

**Della sicurezza.**
- Esiste solo la sandbox di macOS; **Windows e Linux non sono supportati**
  (decisione D-2026-09-30-4). Le protezioni sono forti ma non assolute: "Full
  access" è un'uscita esplicita e visibile dalla sandbox.

**Del processo.**
- **Nessun push, nessuna CI** eseguita sulle ultime 41 modifiche. Suite Rust
  (oltre 1.300 test) e UI (107) verdi in locale all'ultima verifica completa.
- La **mappa dei cancelli nel roadmap è in ritardo**: dice ancora "G1 non
  iniziato", mentre molte voci G1 sono implementate in locale (registro nel
  piano). Va riallineata.
- Ci sono **modifiche non committate di altri** in `docs/` (nuova cartella
  `research/`, README, valutazione, log degli esperimenti): non sono mie e non
  le ho toccate.

## Il principio di prodotto, punto per punto

Il contratto dà dieci principi. Stato oggi:

| Principio | Stato | Nota |
|---|---|---|
| 1. Il modello propone, l'harness tiene i fatti | **rispettato** | log con hash, esiti tipizzati |
| 2. Ogni effetto è limitato allo stesso modo su ogni percorso | **in gran parte** | comandi e file protetti; da confermare in CI e nella passeggiata sull'app nativa |
| 3. Una scrittura si basa su ciò che il modello ha visto | **rispettato** | sovrascrittura legata alla versione letta |
| 4. Una sola semantica di esecuzione | **parziale** | app e console usano lo stesso esecutore; **il valutatore (`eval run`) no** (W2.4 aperto) |
| 5. L'obiettivo non viene compresso | **rispettato** | si ferma e lo dice se non ci sta |
| 6. La verifica afferma solo ciò che ha controllato | **in gran parte** | "non verificato" detto chiaramente; congelamento degli artefatti incompleto |
| 7. Ogni percorso ha un limite | **rispettato** | azioni, rifiuti, verifiche, revisioni e tempo hanno tetti; il tetto di 26 azioni per turno è stato portato a 100 e reso configurabile |
| 8. Misurare il percorso del prodotto | **rispettato dal banco di prova**, **non** dal valutatore ufficiale | il banco di prova usa l'app reale |
| 9. Tenere ciò che si guadagna il costo | **in sospeso** | serve la campagna confermativa |
| 10. I risultati negativi sono risultati | **rispettato** | log degli esperimenti; le prove scartate sono conservate a parte |

## Che cosa manca, in ordine

1. **Misurare sul serio.** Una sola corsia, un modello alla volta, più prove
   per compito, harness congelato, compiti `heldout` mai visti. Prima gli
   interventi anti-loop (ripartenza, nota di riscrittura), poi il confronto con
   un ciclo semplice **a parità di budget** — la prova che il contratto chiede
   (W8.3, W8.4). Senza questo non si può dire che PWR "migliori" il modello.
2. **Chiudere G2**: portare il valutatore sullo stesso esecutore dell'app
   (`EvalHost`), così che una misura ufficiale sia una misura del prodotto.
3. **Chiudere G1 con la CI**: push, CI verde su macOS, passeggiata manuale
   sull'app nativa, e riallineare roadmap e milestone ai fatti.
4. **Qualità dell'uso quotidiano**: avviso prima di cambiare modello in una
   conversazione lunga; scelta di una finestra di contesto stabile per modello
   (oggi calcolata dalla memoria, non misurata); chiarezza sul "non verificato".
5. **Più modelli provati con il codice**: sei dei sette modelli sbloccati oggi
   non hanno ancora una misura sui compiti.
6. **Poi, solo se la misura lo giustifica**: tutto ciò che è adattivo
   (compattazione evoluta, recupero semantico, scelta automatica del modello)
   resta fuori finché la decisione G3 non lo sostiene.

## Cosa serve da te

- **Via libera a una campagna seriale lunga** (circa mezza giornata per i
  modelli principali, altrettanto per i compiti heldout), da eseguire con il
  Mac libero.
- **Se vuoi, un test manuale ora** con l'app ricostruita: i difetti più utili
  scoperti oggi (modelli "Limited", Gemma, Devstral) sono corretti.
- **Una decisione sul push**: finché non lo chiedi non lo faccio, ma senza CI i
  cancelli non si chiudono.
