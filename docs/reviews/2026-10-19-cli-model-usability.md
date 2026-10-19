# Usabilità agentica Gemma — diagnostica CLI

## Perimetro e riproduzione

19 ottobre 2026. Su richiesta del proprietario, nessun nuovo test frontend:
client ACP headless sul percorso `pwr serve --stdio`, un modello alla volta.
Binario e sidecar copiati e congelati prima delle inferenze. Evidenza privata:
`cli-usability-20261019` nel banco privato `audits/linea-gemma4`.
Lì sono conservati protocollo, identità SHA-256, script, richieste/risposte,
tracce MLX, workspace e checker esterni. Nessun progetto manuale originale
è stato modificato. Nessun held-out riservato, download, release o promozione
di default. Le modifiche ai sorgenti e i test PWR sono avvenuti a inferenze ferme.

Due piccoli task di sviluppo, una ripetizione non seeded per condizione:

- **Fixing:** correggere `clamp`, inclusi estremi, valori negativi e bounds invertiti.
- **Creazione:** modulo slug Unicode/ASCII, collisioni anche con nomi già numerati,
  README e test unitari. Il checker funzionale è congelato fuori dal workspace;
  una copia esplicitamente protetta consente al core la verifica dell'acceptance.

La matrice principale è `explicit-contract`: sandbox auto-approve, richieste
eccezionali rifiutate, thinking e assistant spenti, campionamento attuale
1.0 / 0.95 / 64, contesto concesso dal backend, nessun abbassamento artificiale.
Limite per richiesta: 300 secondi, poi cancellazione e chiusura del core.
Una creazione passa solo con checker verde **e** README/test presenti e contratto
immutato. La chiusura del goal è registrata separatamente.

## Risultati della matrice principale

| Pack MLX | Fixing: checker / goal | Creazione: checker / consegna / goal |
|---|---|---|
| E2B 4-bit | Rosso / stallo | Rosso / incompleta / stallo |
| E4B 4-bit | Verde / verificato | Rosso / incompleta / timeout |
| 12B 4-bit | Verde / esito finale non qualificato¹ | Modulo assente / incompleta / timeout |
| 26B A4B 4-bit | Verde / verificato | Verde / mancano README e test / timeout |
| 31B 8-bit | Verde / timeout in revisione | Verde / README e test presenti / timeout |

¹ Il timeout del runner non era una scadenza rigida in presenza di messaggi
continui. Il core di questo caso è stato terminato dal watchdog oltre il budget;
il checker esterno passa, ma tempi e chiusura non sono una misura qualificata.
Il caso è conservato, non sostituito con un successo. Il runner è stato corretto
prima dei casi successivi.

Queste prove non sono una classifica, una misura di affidabilità su repository
reali o un gate di rilascio: task minuscoli, una replica, quantizzazioni diverse,
budget temporale e casi di preparazione separati. In particolare un timeout
non dimostra incapacità di produrre codice corretto. I risultati mostrano
però che codice verde, consegna completa e goal chiuso sono proprietà distinte.

## Profili e cause osservate

- E4B esegue azioni anche nel turno ordinario: prove preliminari con e senza
  check dichiarati superano il checker esterno senza continuazioni umane.
  Un turno ordinario non è limitato a una risposta testuale.
- **Candidato E2B con thinking attivo:** stessi due task/contratto/budget,
  unica variazione il ragionamento. Fixing rosso e stallo; creazione rossa
  e tre completamenti rifiutati. Tentativi di modificare il checker bloccati.
  Nessuna evidenza per promuoverlo; non è una dimostrazione che thinking
  sia sempre peggiore. Il candidato resta nell'evidenza privata.
- E2B promette o dichiara modifiche non confermate dalle azioni; 12B ripete
  formati comando errati. La creazione 12B genera una chiamata di scrittura
  incompleta molto lunga fino alla cancellazione, non un semplice idle.
- 26B ripete scritture identiche correttamente respinte, alternandole a comandi.
  Il requisito di collisione viene infine risolto, ma la consegna non è completa.
- Il costo delle revisioni finali è materiale. La traccia conferma `thinking:false`
  nelle generazioni ordinarie; la revisione indipendente usa il suo ragionamento.
  Non è provato un bug dell'interruttore thinking.
- La prima preparazione inferiva tutti i test come acceptance e bloccava anche
  l'aggiunta dei test richiesti. Risolto nel banco dichiarando il solo artifact
  del proprietario, senza indebolire la protezione del prodotto. I casi precedenti
  restano distinti e non sono sommati alla matrice principale.

## Correzioni apportate dopo riproduzione

1. Runner ACP: deadline monotonic controllata anche quando continuano ad arrivare
   eventi. Test deterministico con coda ancora piena alla scadenza.
2. Executor: una revisione successiva non cancella `delivered` di una modifica
   realmente osservata; le sole risposte in prosa non diventano artifact consegnati.
3. Risposta ACP di stallo: conserva `outcome` del turno invece di ricreare valori
   predefiniti perdendo i check osservati. Non certifica l'acceptance per deduzione.
4. Tre verifiche identiche fallite non vengono più attribuite probabilisticamente
   a permessi o ambiente: il messaggio chiede di esaminare output e implementazione.

Validazione: 54 test executor, regressione ACP dello stallo e 27 test runner
riusciti; build release della CLI aggiornata riuscita. Checker protetti rimasti
identici alle copie esterne in tutti i dodici casi principali/candidati.
Nessun processo d'inferenza rimasto attivo alla chiusura.
Le inferenze della matrice usano il binario congelato precedente:
queste correzioni hanno prova automatica, non una nuova qualificazione con modelli
o nell'app nativa. Nessuna nuova certificazione del frontend.

## Recupero successivo: comandi e no-op

Implementato e verificato dopo la matrice iniziale:

- `RefusalStreak` non dimentica una scrittura rifiutata quando riesce una
  lettura o un check estraneo. Il conteggio resta distinto per fingerprint;
  un retry non rifiutato libera la propria voce e una modifica osservata
  degli input riapre il recupero, inclusa la precedente segnalazione di loop.
- Replay del turno reale: scritture identiche alternate a letture ora terminano
  con `NoProgress` prima di 20 generazioni. Controesempio riuscito: dopo una
  vera modifica del file un nuovo recupero può arrivare al completamento.
- Per Python/Node, uno script esistente seguito da argomenti uniti in una
  singola stringa riceve un rifiuto con esempio JSON di argv separato.
  Nessuno split o esecuzione automatica. Preservati script realmente nominati
  con spazi e comandi correttamente separati. È un controllo mirato, non un
  parser generale di righe shell o una soluzione a ogni errore di schema.

Test: 304 unit test orchestrator riusciti (2 ignorati), 9 recovery, 12 loop,
56 avversariali tools; build release CLI, formattazione e diff check riusciti.

Due repliche 26B A4B 4-bit della creazione, con task/profilo/budget invariati,
nuovo binario congelato e sidecar precedente:

| Replica | Esito |
|---|---|
| recovery-r1 | Protocollo fallito: tre chiamate incomplete, zero azioni, circa 15 s. Il nuovo recupero non è stato esercitato |
| recovery-r2 | Consegna completa e checker esterno verde; goal accettato, 23 azioni totali, circa 250 s, sandbox attiva |

Nella seconda replica README coerente, test unitari presenti e due test
generati dal modello eseguiti indipendentemente con esito positivo. Checker
del proprietario immutato in entrambe. I sei tentativi falliti della seconda
replica restano nella traccia: successo finale non significa percorso senza
errori. La precedente prova produceva il modulo corretto ma lasciava assenti
README/test e raggiungeva il timeout.

Due osservazioni non dimostrano un aumento della probabilità di successo:
campionamento non seeded e un fallimento di protocollo restano. Il beneficio
del rilevamento no-op ha una regressione deterministica; il miglioramento
della consegna ha per ora una sola osservazione positiva. Nessun profilo
o campionamento promosso. Evidenza privata `recovery-r{1,2}`, identità
`recovery-r1-identity.json`, riepilogo `recovery-summary.json`.

Limite del banco: il check dichiarato al core copre le funzioni; presenza
di README/test è verificata anche dal controllore esterno. Un goal verde
da solo non viene contato come consegna completa. Prima di campagne più
grandi, codificare tutti i requisiti automatici nel contratto, includendo
test in sottocartelle e convenzioni di nome valide.

## Prossima esecuzione

1. S03/S04: isolare il protocollo nativo, inclusi `path` omesso e forme come
   `path:="."` osservate nelle nuove repliche. Verificare renderer/schema e
   feedback con fixture prima di modificare template o profili. Rimane aperto
   il recupero dei check-in senza modifiche e dei comandi malformati non coperti.
2. Separare budget e risultati di esecuzione, verifica e revisione. Provare un
   solo candidato alla volta; nessuna eliminazione automatica della revisione
   o del turno ordinario, nessuna semplice estensione dei limiti.
3. Ripetere fixing e creazione sul nuovo binario, poi task dev più realistici
   multi-file/framework con acceptance semantica, repliche e stesso protocollo.
4. Solo dopo, confrontare i pack QAT e profili per modello. Occorre beneficio
   replicato prima di proporre un default. S03/S04 restano aperti; niente gate DONE.
