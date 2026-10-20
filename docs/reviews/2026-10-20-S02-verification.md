# S02 — Verifica indipendente e correttezza

Avvio 2026-10-20; **chiusura S02 DONE** nel [rapporto finale](2026-10-20-S02-closeout.md). Le sezioni seguenti conservano le tappe intermedie. S01 chiuso; nessun gate o default modello
modificato. Evidenza privata `sprint-S02-20261020`, protocollo definito prima
delle modifiche. Nessuna inferenza e nessun task riservato impiegato.

## Esempi attesi prima della correzione

| Caso | Esito atteso |
|---|---|
| unittest dichiara un fallimento, wrapper termina con exit 0 | Fallimento dei controlli; exit originale conservato |
| cargo/pytest dichiarano test falliti, wrapper termina con exit 0 | Fallimento, senza promozione a verified |
| Il controllo B era nella baseline e non viene rieseguito | Baseline non dimostrata preservata |
| Build senza test, oppure prosa che menziona errori | Nessun fallimento test inventato dalla sola parola error/failed |
| Tutti i check dichiarati passano, ma manca il requisito ASCII/bool/precisione | Il pass non dimostra quel requisito; occorre controllo indipendente esplicito |

La correzione non deve riscrivere exit code, output o evidenze storiche. Usa i
riepiloghi riconosciuti dei runner e mantiene i limiti dei formati non riconosciuti.
Non è una difesa universale contro un checker deliberatamente mendace.

## Lavoro successivo dello sprint

- Collegare identità dei file, argv, baseline ed esiti; invalidare prove obsolete.
- Congelare regressioni dev per Unicode/precisione/bool/atomicità già osservati.
- Checker modificati, verifiche assenti/zero/interrotte e copertura semantica.
- PDF-01: risorse/font per pagina e ToUnicode, con limiti di espansione/memoria;
  allegato fino al contesto. Nessun fallback dipendente da programmi di questo Mac.


Per distinguere assenza di prova e test fallito, una baseline con controlli non
rieseguiti avrà stato `incomplete` e lista `missing_checks`, separata dai nuovi
fallimenti osservati. Esempio di messaggio: «Baseline comparison incomplete:
checks were not rerun». Il turno rimane non verificato; non si afferma che il
controllo omesso sia stato eseguito o sia fallito. È una correzione del significato
degli esiti, senza ridisegno dell’interfaccia.

## Primo intervento implementato e verificato

I riepiloghi riconosciuti di unittest, cargo, pytest, Node e Vitest/Jest impediscono
ora che un wrapper con exit 0 nasconda test dichiarati falliti. Il risultato grezzo
(exit, stdout, stderr) resta intatto. Il criterio è condiviso fra verifica, turni
singoli, Goal mode e riepilogo CLI. I formati non riconosciuti e l'autenticità di
un checker restano limiti espliciti.

Un controllo presente nella baseline ma omesso dalla nuova esecuzione produce
ora `incomplete` con `missing_checks`, senza inventare un test fallito. Il lavoro
rimane non verificato anche davanti a un esito di accettazione contraddittorio;
annullamento e altri arresti mantengono la propria precedenza.

Evidenza privata nella directory di audit `sprint-S02-20261020`:

- `masked-before.log`: due regressioni falliscono prima della correzione.
- `typed-verification-suites.log`: domain, verify, orchestrator e CLI, 985 test
  riusciti e due ignorati che richiedono checkout predisposti.
- Dopo gli ultimi cambiamenti circoscritti: suite CLI completa, 284 riusciti
  (`cli-final.log`); integrazione completion, tre riusciti (`completion-final.log`);
  regressioni wrapper/baseline, quattro riusciti (`masked-final-r2.log`).
- Frontend: 193 test riusciti e build riuscita (`frontend-outcomes.log`,
  `frontend-build.log`). Nessuna prova di rendering nativo dedotta da jsdom.
- Clippy finale sui quattro crate, tutti i target: riuscito
  (`clippy-final-r2.log`); formattazione e controllo whitespace riusciti.

I conteggi delle suite complete e delle riesecuzioni mirate non si sommano.
Le esecuzioni intermedie sono conservate e non sostituiscono quelle finali.
Nessuna campagna di inferenza è stata eseguita.

## PDF originale: difetto ancora aperto

Il PDF originale di 17 pagine è invariato, SHA-256
`956c58dd8b29e60c37042ffddb30818d02b499d72058a8a6bcd808218ddf0f58`.
L'estrattore attuale rifiuta ancora il documento con `Undecodable`; il riferimento
indipendente estrae 34.108 caratteri. L'esempio CLI termina con exit 0 anche
quando stampa il rifiuto: tale exit non costituisce prova di estrazione riuscita.
Non sono stati introdotti fallback locali né allentati i controlli di plausibilità.

S02 resta **IN_PROGRESS**. Questo intervento risolve due falsi positivi concreti;
provenienza completa delle prove, regressioni semantiche e lettura PDF rimangono
necessari prima della chiusura.

## Criteri congelati per il completamento (2026-10-20)

Prima dell'implementazione restante:

- Una verifica registra argv separati, file nello scope con hash, configurazione
  di esecuzione e snapshot prima/dopo. Se lo scope cambia durante i controlli,
  oppure dopo prima del riuso, il verde grezzo resta nel log ma non certifica
  lo stato nuovo. File illeggibili o limiti superati impediscono la certificazione.
- Lo scope è esplicito: file sorgente non ignorati, dichiarazione `.pwr/checks.json`
  e file locali passati direttamente nei comandi; esclusi dipendenze, build,
  cache e stato interno. Non certifica servizi esterni o cambiamenti transitori
  ripristinati fra snapshot. Il confronto della baseline collega i due ID.
- Regressioni dev nuove (nessun task riservato): ASCII contro cifre Unicode,
  interi oltre 2^53, booleani non numerici e rifiuto atomico di un batch invalido.
  Un checker superficiale deve passare il controesempio, quello indipendente
  deve fallire. Un positivo corretto deve passare entrambi.
- PDF: pagine/font reali, mappe Unicode e cambio font fra pagine; rifiuto di
  mappe mancanti/corrotte, scansioni e bombe di decompressione. Il documento
  originale deve arrivare come testo nel percorso allegato, senza inferenza.
  Nessuna promessa di OCR, ordine visivo o supporto universale ai PDF.
