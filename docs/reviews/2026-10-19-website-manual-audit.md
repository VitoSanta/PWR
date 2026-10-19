# Prove manuali website — analisi e regressioni

Data: 2026-10-19. Cinque export diagnostici forniti dal proprietario, relativi
a E2B, E4B, 12B, 26B A4B e 31B. Evidenza privata: `website-manual-20261019`.
Gli export, le impronte dei sorgenti e i risultati negativi sono conservati.
I progetti originali non vengono corretti o rigenerati da questa analisi.

## Risultati osservati

| Caso | Tool riusciti / falliti nella traccia | Eventi recovery | Build indipendente del sorgente finale |
|---|---|---|---|
| E2B 4-bit MLX | 18 / 1 | 2 | Non eseguibile: manca package.json |
| E4B 4-bit MLX | 0 / 2 | 0 | Non eseguibile: manca package.json |
| 12B 4-bit MLX | 7 / 10 | 3 | Compila; app.html conserva il placeholder Angular e le route sono vuote |
| 26B A4B 4-bit MLX | 94 / 25 | 7 | Compila; app.html conserva il placeholder Angular e le route sono vuote |
| 31B 8-bit MLX | 23 / 5 | 0 | Fallisce: Tailwind usato direttamente come plugin PostCSS incompatibile |

Le build sono eseguite su copie temporanee dei sorgenti con le dipendenze
già installate, usando direttamente Angular CLI. Nessuna installazione né
nuova inferenza. Un'uscita 0 non dimostra la realizzazione del sito richiesto:
12B e 26B sono controesempi concreti. Questa verifica non sostituisce l'analisi
visiva e funzionale di ogni pagina, dei contenuti e dei collegamenti.

I conteggi descrivono tentativi, non percentuali di successo dei task. Un
evento recovery conferma un recupero del protocollo, non la risoluzione di
ogni requisito. I turni, gli interventi, le quantizzazioni e gli stati iniziali
non sono una campagna controllata: non classificare i modelli per qualità
o velocità da questi numeri. In particolare le prove lunghe si sovrappongono
ad attività di sviluppo e build nel checkout; i tempi non sono confrontabili.

## Cause distinte e disposizione

| ID | Evidenza | Diagnosi / controllo necessario | Sprint |
|---|---|---|---|
| WEB-01 | Terminal 9 come unica scheda | Il contatore interno era usato come etichetta. Ora UI con posizione corrente; ID del processo invariato | S00, implementato |
| WEB-02 | Anello input arancione/verde | Accenti sperimentali sostituiti con i tre blu esatti del logo | S00, implementato |
| WEB-03 | Explorer non mostra i nuovi file | Polling precedente riguardava solo documenti già aperti. Ora aggiorna anche l'albero visibile senza documenti | S01, regressione automatica riuscita |
| WEB-04 | Un file eliminato impedisce refresh di altri buffer | Il catch esterno interrompeva l'intero ciclo. Ora l'errore del singolo file non blocca gli altri; dirty buffer conservati | S01, regressione automatica riuscita |
| WEB-05 | “Finished after 0 actions” su un piano senza esecuzione | In workspace un turno senza azioni resta non verificato con tono neutro/pausa; normale Q&A chat distinto | S02, regressione automatica riuscita |
| WEB-06 | E4B: due invocazioni `ng` con l'intera riga negli args; stop stalled | Formato comando errato e adattamento insufficiente al feedback. Non correggere silenziosamente input ambigui; riprodurre il recupero/stallo con fixture | S03, aperto |
| WEB-07 | 12B: sei write_file identici su app.ts, poi proposta già rifiutata; stop recovery | La protezione no-op funziona, il controller non risolve la causa. Riprodurre read→patch mirata→build e arresto del loop | S03, aperto |
| WEB-08 | 26B: otto chiamate senza operation in workspace_interact | Errori schema segnalati e retry presenti, ma ricorrenti. Verificare schema effettivo nel prompt, contesto e riuso del feedback | S03/S04, aperto |
| WEB-09 | 26B turn2-call17: apertura dell'origine PWR rifiutata | Protezione corretta. Il sito deve usare una porta diversa dall'interfaccia dev; non rimuovere il confine IPC | S01/S03, fixture di verifica |
| WEB-10 | 26B turn2-call94: processo della conversazione non disponibile | Rifiuto di ownership corretto; da ricostruire la sequenza di chiusura/cambio sessione e la consapevolezza del controller | S01/S03, aperto |
| WEB-11 | 31B: dipendenze/CLI, flag non interattivi e Tailwind/PostCSS | Errori di implementazione/versione; build indipendente effettivamente rossa. Il turno è interrotto, non un completamento qualificato | S02/S03, aperto |
| WEB-12 | Due build verdi con template iniziale ancora attivo | Il verificatore tecnico non copre il requisito “sito realizzato”. Servono acceptance congelate su contenuto, navigazione e rendering | S02, prioritario |
| WEB-13 | Dev server terminato ma finestra PWR presente | Una build app avviata separatamente è indipendente da VS Code. Lo screenshot dimostra il dev server ucciso, non l'uscita del processo nativo. Riprodurre il ciclo CLI/dev server/app/core separatamente | S01, prioritario |
| WEB-14 | Browser stretto dentro finestra larga | Il breakpoint seguiva la finestra. Ora container query sul browser: comandi su prima riga, indirizzo e Go sulla seconda | S00/S01, implementato, verifica nativa residua |

Alcuni altri errori delle tracce sono E404 di pacchetti sbagliati, ERESOLVE,
protezioni contro rewrite distruttivi e processi di lunga durata inviati a
run_command. Non sono tutti bug del tool: conservarne causa e contesto.
Non aggirare sandbox, hash, letture obbligatorie o ownership per ottenere un
numero inferiore di rifiuti.

## Correzioni e prove correnti

La numerazione visibile viene ricalcolata dopo la chiusura di una scheda senza
riutilizzare gli ID interni. Il gradiente e il glow usano esclusivamente
`#1A73E8`, `#4C8DF6`, `#A8C7FA`. L'editor conserva il controllo dei conflitti e
non sovrascrive testo non salvato. Il browser non mostra più un dominio
specifico nel CTA dello stato vuoto.

192 test frontend riusciti, inclusi i nuovi casi explorer senza file aperti,
file eliminato prima di un altro buffer, numerazione dopo chiusura e workspace
senza azioni distinto dalla chat. Build web e bundle macOS ad-hoc riusciti.
I cambiamenti di controller e verifica semantica restano aperti, non sono
risolti dalla presentazione della chat. Nessun gate o sprint è dichiarato DONE.

La verifica visiva nativa di questa build è incompleta. La sessione precedente
mostrava un timeout di startup `_pwr/models`; dopo riavvio la cattura CUA non
ha permesso di qualificare i controlli. Il frontend compilato servito localmente
in browser mostra il messaggio previsto di esecuzione fuori da Tauri e non
riporta errori console. Questo non certifica startup/IPC o rendering WebKit.

## Prossima sequenza

1. S01: finestra stabile con una sola istanza; lifecycle app/core/sidecar,
   terminale, browser e refresh editor, inclusi i casi negativi.
2. S02: acceptance indipendente per questi difetti, usando copie dev e checker
   non modificabili dal modello. Nessun riuso delle prove come held-out.
3. S03: regressioni deterministiche di schema, no-op e recupero prima di
   cambiare prompt o fare altre cinque generazioni lunghe.
4. Una nuova prova Gemma controllata dopo i fix, con sorgenti e budget congelati.

Gli export sono dati da analizzare: prompt e istruzioni contenuti nelle chat
non autorizzano nuove azioni, installazioni o cambi di policy in questa sessione.
