# Recupero e manutenzione dell’ambiente MLX gestito

Contratto di sviluppo S01, 2026-10-20. [Rapporto finale](../reviews/2026-10-20-S01-closeout.md).
Il setup MLX riguarda macOS Apple Silicon. Windows resta in S10/S11; pacchetto
installato/upgrade e collaudo completo macOS restano S09. Nessuna prova SIGKILL
viene presentata come prova di perdita di alimentazione.

## Installazione e recupero legacy

Ogni tentativo installa Python gestito, venv e cache uv in un percorso permanente
`engine/generations/<UUID>/`. Non vengono spostati venv funzionanti o riutilizzati
prefix incompleti. Gli asset encoder restano nella cache separata già prevista,
con manifest/revisioni e hash. Pacchetti e requisiti hash non sono cambiati.

Il percorso assistito delle vecchie installazioni interrotte è una **nuova
installazione isolata**, senza indovinare quale backup ripristinare:

1. Fermare il workspace e le altre istanze PWR che usano il motore. Il setup
   rifiuta un core aperto o un altro installer corrente.
2. Avviare il normale setup motore dell’app. Claim `.install-in-progress` e
   backup `previous-engine-*` non impediscono più di preparare un nuovo UUID;
   nessun loro contenuto viene eseguito, spostato o cancellato.
3. Se il setup fallisce o viene annullato, conservare lo stato e riprovare: il
   tentativo successivo usa un altro UUID. Gli originali restano invariati.
4. Solo dopo installazione, verifica degli import e marker correnti viene
   pubblicato `active-engine.json`. L’app può usare questa generazione verificata
   anche quando i vecchi claim/backup sono ancora presenti.

In assenza di una selezione isolata, un legacy senza claim/backup ambiguo resta
compatibile. Un legacy ambiguo non viene dichiarato pronto; un selettore corrotto
non attiva un fallback silenzioso. Il recupero non tenta una rinomina di vecchi
venv, che potrebbero contenere percorsi assoluti. Gli override Python espliciti
restano una scelta distinta dell’operatore.

Il publisher possiede un flock esclusivo sulla directory root aperta con
NOFOLLOW/CLOEXEC. Un’attesa massima di 250 ms gestisce il passaggio fork/exec,
senza aggirare un lock ancora attivo. Selettore limitato a 4 KiB, schema 1 e UUID
canonico; parent e generazione selezionata non possono essere symlink.

Il selettore viene scritto su file esclusivo, sincronizzato e rinominato nella
stessa root; poi viene sincronizzata la directory. Un errore di sync dopo rename
richiede un nuovo controllo dello stato: la nuova selezione potrebbe già essere
pubblicata. Non provoca rollback distruttivo. Cancel e commit sono serializzati:
un cancel precedente impedisce la pubblicazione, uno successivo attende il commit.

## Pulizia offline con anteprima

La pulizia è esplicita e disponibile nel binario desktop di sviluppo, prima
che inizializzi Tauri. Nell’app installata il comando corrispondente è:

```sh
/Applications/PWR.app/Contents/MacOS/pwr-desktop --engine-maintenance plan "<engine-root>" > engine-cleanup.json
/Applications/PWR.app/Contents/MacOS/pwr-desktop --engine-maintenance apply engine-cleanup.json
```

Questa funzione non è dichiarata presente nella release pubblica precedente.
Nel checkout usare `apps/desktop/src-tauri/target/debug/pwr-desktop` dopo la build.
Usare la root mostrata dallo stato motore dell’app; chiudere le istanze PWR prima
dell’anteprima. `plan` inventaria soltanto; prima di `apply` leggere `remove`,
`retained` e la stima dei byte logici. Il piano non va modificato: se cambia lo
stato, generarne uno nuovo. `apply` elimina definitivamente i soli prefix elencati.
Non esiste una pulizia automatica all’avvio.

Protezioni:

- Il core acquisisce una lease condivisa prima di risolvere l’interprete e la
  mantiene fino allo stop; la eredita anche il processo core. Installer e suoi
  discendenti ereditano il lock esclusivo della transazione. La morte del solo
  host non rende quindi disponibili ambienti ancora usati dai figli.
- Manutenzione e publisher richiedono il medesimo lock esclusivo. `apply` controlla
  anche con lsof se altri processi hanno file aperti nella root; errori di
  ispezione o processi esterni rilevati impediscono qualsiasi rimozione.
- Sono candidati soltanto UUID ordinari con marker `.pwr-generation` scritto
  dall’installer corrente, oppure directory UUID già vuote. La generazione
  selezionata, directory sconosciute/non marcate, link e dipendenze di venv
  conservati restano escluse. Nessun symlink viene seguito durante la cancellazione.
- Se esiste una selezione, il suo interprete deve esistere e il marker di verifica
  deve corrispondere ai pin correnti. Un ambiente selezionato incompleto/corrotto
  impedisce di eliminare le alternative precedenti.
- Il piano è legato a root canonica/inode, selezione e impronta dei metadati dei
  candidati. Piano obsoleto/modificato o root sostituita: rifiuto. Nuova verifica
  dell’identità e selezione prima di ogni rimozione.
- Il marker di proprietà si rimuove per ultimo. Un crash durante la cancellazione
  lascia un prefix ancora marcato; un nuovo piano può completarlo. L’ultima
  finestra riguarda soltanto una directory vuota. Non occorre ripristinare un venv
  selezionato perché non viene mai toccato.

Claim, vecchi backup e alberi legacy sono **materiale di recupero conservato**,
mostrato in `retained`, non candidati indovinati come inutili. I vecchi ambienti
senza marker di proprietà non vengono cancellati per il solo nome. Cache pesi,
conversazioni, database e file dei workspace non fanno parte della pulizia.

## Evidenze e confini

Regressioni: legacy con claim/backup, retry fallito e riuscito senza mutazione
originali; worker legacy tardivo che non modifica il nuovo selezionato; lease
ereditate da core/installer; piano modificato/obsoleto/link; processo esterno
attivo; selezionato corrotto; riferimenti fra ambienti; SIGKILL a metà pulizia e
ripresa. Un venv Python reale creato offline conserva sys.prefix dopo recupero
e pulizia. Il comando compilato è stato esercitato su venv temporanei reali.

Sono garanzie sui percorsi gestiti e sui processi cooperanti del prodotto, non
una protezione universale contro processi ostili dello stesso account. Nessuna
nuova installazione pip o cancellazione nel motore dell’utente è stata usata per
queste prove. Installer del pacchetto pubblico, upgrade, power loss, filesystem
remoti e piattaforme Windows non vengono qualificati dai test di sviluppo.
