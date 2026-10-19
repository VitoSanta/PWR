# S00 — Correzioni dopo la prova manuale macOS

Data: 2026-10-19. Stato: implementazione verificata con test automatici;
ricontrollo nativo parziale, come descritto sotto.

Il proprietario conferma durante la prova che salvataggio dei file, protezione
delle modifiche non salvate e terminale funzionano. Questi riscontri riguardano
il candidato provato e non chiudono l'intera campagna end-to-end.

| Segnalazione | Correzione | Verifica residua |
|---|---|---|
| QAT separato dal modello base | Raggruppamento E4B/26B con QAT come variante; riferimenti operativi conservati | Menu e Model Manager nativi |
| Model Manager non scorre | Contenitore flessibile con altezza minima zero e overflow verticale | Scorrimento touchpad fino all'ultima variante |
| Auto-approve cambia altezza | Spazio stabile riservato alla descrizione | Confrontare gli stati alle dimensioni supportate |
| Barra trasparente dopo uscita fullscreen | Callback AppKit ricontrollano lo stato corrente anche dopo la transizione | Ripetere entrata/uscita fullscreen |
| Tool massimizzato sopra il pulsante conversazione | Spazio superiore per i tool massimizzati | Finestra normale, fullscreen e stretta |
| Branding esplicito nei controlli | Testi prodotto e nome del tema aggiornati; ID persistiti conservati | Controllo visivo; attribuzioni e licenze restano separate |
| Editor: cursore disallineato e stili mancanti | CSS Monaco nel bundle iniziale, font monospace stabile e ricalcolo dopo il caricamento font; stato cursore per file | Python/TypeScript, selezione, click, scroll, cambio file e temi |
| Terminale precedente riappare aprendo l'editor | Al caricamento si esclude la scheda terminale e la sua massimizzazione salvata | Aprire terminale, uscire, rilanciare e aprire solo Editor |

Il test di regressione della sessione verifica che la scheda terminale salvata
non venga ripristinata, che aprire Editor mostri soltanto Editor e che un nuovo
terminale possa comunque essere aperto manualmente.

Verifiche sul nuovo sorgente: 184 test frontend riusciti in 25 file; build
Angular di produzione riuscita; Clippy nativo con warning negati riuscito.
Il CSS Monaco porta il bundle iniziale a circa 1,58 MB: rimane il warning
del budget di 1 MB, senza modifica della soglia. I warning canvas di jsdom
restano un limite dei test sintetici, non una prova del rendering nativo.

Nessuna nuova inferenza, confronto Harness², release, bump versione o firma
Apple. S00 e il ricontrollo macOS restano aperti finché le prove native sopra
non sono registrate.

## Riscontri nativi sul bundle aggiornato

Bundle macOS ad-hoc ricostruito e rilanciato con successo, versione invariata
0.4.1-alpha. Prova eseguita: apertura Editor e Terminale, uscita normale
dall’app, riavvio e apertura del solo Editor. L’albero accessibilità mostra
soltanto la scheda Editor: il vecchio Terminale non ricompare.

Aperto anche hello.py: numeri di riga e colorazione Python sono visibili,
la textarea di input non appare più come un campo separato nell’angolo.
Questo riscontro non sostituisce la prova approfondita del cursore e della
selezione. Gli altri controlli visivi nella tabella restano da completare.
Log automatici conservati nell’evidenza privata S00, sottocartella ui-fixes.

## Secondo riscontro del proprietario — coerenza visiva

Il proprietario respinge la fascia superiore aggiunta al tool: sostituita con
la rimozione visiva e dalla navigazione a tastiera del pulsante conversazione
quando il tool è massimizzato o occupa la pagina. Resta soltanto lo spazio
richiesto dai controlli nativi macOS in modalità finestra.

Correzioni condivise: campi di testo e select arrotondati con lo stesso token;
textarea della stessa famiglia; pannelli select con superficie, elevazione e
righe coerenti ai menu; riga Options a due linee con altezza automatica e
padding, senza testo tagliato. La descrizione Auto-approve è stabile nei due
stati e non richiede più tre righe vuote riservate in ogni controllo.
La freccia sinistra usa il glifo destro locale specchiato, evitando il simbolo
assente dal subset. Regole registrate in docs/design-system.md.

184 test frontend passano; nuova build web e bundle macOS ad-hoc riusciti.
Confermati visivamente i campi arrotondati del profilo. La verifica CUA degli
altri pannelli è parziale: durante le osservazioni sono comparsi errori di
cattura ScreenCaptureKit e stati di transizione incompleti. Non attribuire
un passaggio visivo completo a questi tentativi; ricontrollare massimizzazione,
menu, select e freccia nella sessione manuale attiva.

## Ordinamento dei modelli

Richiesta del proprietario: ordine crescente E2B, E4B, 12B, 26B, 31B.
Applicato nella funzione condivisa da selettore e Model Manager. La dimensione
è quella base (26B, non il suffisso A4B); QAT e quantizzazioni restano varianti.
Il test usa riferimenti in ordine mescolato e verifica anche il raggruppamento
QAT. Suite frontend: 185 test riusciti in 25 file.

Bundle ricostruito e riavviato. Il selettore nativo mostra nell’ordine E2B,
E4B, 12B, 26B A4B, 31B, con scelta variante per E4B e 26B.

## Cronologia centrata sull’esito — 2026-10-19

Il proprietario approva la distinzione tra tentativi intermedi e risultato.
Nelle viste Detailed/Compact il numero di chiamate non eseguite non produce
più un badge rosso né un triangolo sul gruppo di azioni. La cronologia
espandibile conserva chiamate, errori, rifiuti e retry; Raw Trace resta integra.
I retry esplicitamente esauriti restano evidenti. Il recupero richiede un
evento recovery del motore: un comando successivo riuscito non basta.
L’ultimo evento retry/recovery prevale, quindi un recupero successivo non
rimane oscurato da un retry precedente fallito, né nasconde un nuovo arresto.

La dicitura temporale diventa “Run ended”; i criteri di verifica e le evidenze
di runOutcome non cambiano. Nessuna chiamata viene rimossa dai dati, nessun
esito fallito viene trasformato in successo e nessun comportamento del motore
o delle policy viene modificato. Test dedicati coprono assenza di evidenza,
recupero confermato, nuovo fallimento e retry attivo/interrotto.

Verifica automatica: 189 test frontend riusciti e bundle macOS ad-hoc
ricostruito e riavviato. Nuova presentazione delle conversazioni storiche
da ricontrollare visivamente: nessuna nuova inferenza eseguita.

## Collasso e distinzione tra finestra e fullscreen

Nuova segnalazione del proprietario: il collasso dell’unico tool massimizzato
ripristinava la scheda senza piegarla; il titolo diventava poi statico. Erano
due limitazioni esplicite del codice, ora rimosse. Ogni titolo resta un
pulsante, il collasso esce dalla massimizzazione e modifica lo stato della
scheda; tutte le schede possono essere collassate e tale stato è conservato
al caricamento, senza apertura automatica di un’altra scheda.

Il pulsante conversazione viene nascosto dal tool soltanto quando anche
l’app è in fullscreen. In modalità finestra macOS resta accanto ai controlli
nativi. Clearance superiore dei tool ridotta da 44 a 36px, con controllo
conversazione da 28px per mantenere le aree separate.

191 test frontend riusciti, inclusi collasso/riespansione dell’unico tool,
più schede collassate e ripristino dello stato. Build web riuscita; verifica
nativa delle combinazioni finestra/fullscreen ancora da registrare.

Bundle aggiornato e riavviato. Verifica nativa riuscita su Review come unica
scheda: massimizzazione → collasso (ritorna la conversazione) → espansione
→ nuovo collasso. Il titolo resta sempre un pulsante. Con tool massimizzato,
entrata nel fullscreen macOS rimuove Conversations dall’albero accessibilità;
uscita dal fullscreen lo ripristina. Screenshot finale in finestra conferma
pulsante chat sopra il tool e spaziatura superiore ridotta. Nessuna inferenza
e nessuna modifica ai file del workspace della prova.

## Accettazione frontend del proprietario

2026-10-19: il proprietario conferma «per il momento con il frontend ci siamo
è apposto». Il ciclo corrente di correzioni UI è accettato; non sono richieste
altre modifiche estetiche in questa fase. Questa accettazione non equivale
al collaudo E2E finale S09, a un audit accessibilità completo o alla chiusura
degli altri criteri S00. Si prosegue con residui tecnici e contratto 0.5.
