# Contratto candidato 0.5 — bozza da completare in S00

Data: 2026-10-19. Stato: DRAFT, non autorizza release né modifica i gate.

## Proposta di chiusura S00 — 2026-10-20

Da approvare dal proprietario; non è una decisione già assunta.

1. Chiudere S00 come consolidamento della baseline di sviluppo identificata,
   suite locali, inventario e backlog riconciliato. Non è un freeze release,
   qualificazione nativa completa o chiusura dei difetti assegnati a S01–S04.
2. Conservare il target possibile 0.5.0 e la successiva 1.0 basata su uso esterno.
   Nome/canale definitivi restano la decisione S12 già prevista; nessun bump ora.
3. Mantenere Gemma-only e tutti gli otto pack MLX del registro come candidati
   da qualificare, senza dichiararli tutti affidabili per coding. Il supporto
   pubblicizzato richiederà la matrice misurata S04/S07. Nessun default cambia.
4. Usare il Mac attuale come host di sviluppo misurato e il PC Windows indicato
   sotto come target S10/S11. Non inferire compatibilità con macOS più vecchi
   o con altri hardware. I requisiti minimi pubblici saranno qualificati in S04/S10.
5. Conservare i 34 scenari come base. Specifiche/checker, repliche, budget e
   criteri quantitativi devono essere approvati e congelati prima delle campagne
   S04/S05/S07; valori mancanti non diventano permissivi per il solo passaggio
   di S00. Questa disposizione sposta esplicitamente la loro finalizzazione
   dagli obblighi generici di S00 alle fasi che possono fondarla su dati.
6. Nessuna deroga G3/F6: restano necessari prima di dichiarare pronti release
   e Windows secondo il contratto vigente. Hosted CI e packaging sul candidato
   spettano a S07/S08; firma/account Apple restano S12 dopo Windows.
7. Startup/IPC, controlli nativi residui e PDF Unicode rimangono lavoro aperto
   S01/S02; protocollo/recupero modelli S03/S04. Il collaudo completo resta S09.
   Non dichiarare questi problemi risolti né nascondere i B0/B1 nel collaudo.

La proposta rende espliciti i confini e risolve la dipendenza circolare tra
chiusura S00 e soglie che richiedono le baseline S04/S05. Finché non approvata,
S00 rimane IN_PROGRESS, anche se il consolidamento tecnico è terminato.

## Host identificati

| Ruolo | Configurazione | Stato |
|---|---|---|
| Sviluppo macOS | MacBook Pro Mac14,5, Apple M2 Max 12 core, 64 GB RAM; macOS 27.0.1 build 26A434 | Inventario locale; non qualifica ogni Mac o versione macOS |
| Target Windows | Windows 11, Intel i7, 32 GB RAM, NVIDIA GeForce RTX 4080 16 GB VRAM | Dichiarazione del proprietario 2026-10-20; modello CPU, build OS, driver e backend da rilevare/verificare in S10 |

## Decisioni già assunte

- Prodotto soltanto Gemma 4; altre famiglie sono benchmark esterni. F6 è già
  revisionato da D-2026-10-18-2: non riaprire la scelta multi-family.
- Sviluppo e qualificazione macOS precedono Windows. Account Apple,
  Developer ID e notarizzazione sono differiti a S12, quando disponibili.
- Obiettivo possibile 0.5.0; la 1.0 attende esperienza d'uso esterna.
- Il proprietario ha accettato l'iterazione frontend corrente. Le verifiche
  native mirate non sostituiscono S09 e S11.

## Proposta di accettazione funzionale

Usare i 34 scenari FX/CR/UX/RC/DS della guida sprint, con specifiche e checker
congelati prima di S07. Si propone che ogni caso obbligatorio debba passare;
un caso non eseguito resta non qualificato. Registrare il primo esito negativo,
la correzione e il retest, senza sostituire la storia con il risultato finale.

Condizioni proposte: nessun difetto bloccante aperto di perdita dati, scrittura
non autorizzata, evasione dal workspace, falsa verifica, upgrade corrotto o
processi non governati. Una limitazione richiede disposizione esplicita; una
media di successo non compensa una violazione critica.

Per ogni caso: modello/revisione/quantizzazione, backend e dipendenze, host,
prompt, budget, numero di repliche, interventi ammessi, comandi/checker,
baseline, file finali e risultato indipendente. Niente task riservati usati
come prove dev. Le stesse specifiche funzionali vanno ripetute su Windows.

## Informazioni ancora da fissare prima delle campagne

| Voce | Lavoro richiesto | Sprint |
|---|---|---|
| Modelli supportati | Elenco esatto dei pack/revisioni qualificabili; distinguere caricamento da affidabilità coding | S00/S04 |
| Host | Inventario Mac effettivo, macOS minimo qualificato; host Windows, GPU/driver, RAM/VRAM e backend | S00/S04/S10 |
| Task e repliche | Specifiche dei 34 scenari, stack target, repliche e budget per classe | S00/S07 |
| Prestazioni | Misurare baseline; proporre limiti di memoria, latenza e responsività prima del confronto | S04 |
| Beneficio dell'harness | Margini, potenza, competitori eleggibili e baseline minima secondo G3/F6 | S05/S07 |
| Distribuzione | Canale finale, upgrade di partenza, backup chiave updater e prove del feed | S08/S12 |

I valori mancanti non sono implicitamente approvati. Il catalogo dei modelli
non equivale a questa matrice. Nessuna soglia di successo o latenza viene
inventata dopo aver visto gli esiti.

## G3/F6 e release

Resta in vigore il contratto corrente: questa bozza non separa automaticamente
una beta pubblica dal requisito di confronto confermativo. In assenza di una
nuova decisione esplicita, servono i gate esistenti. La possibilità di criteri
distinti per una beta è una decisione futura, non una deroga già concessa.

Fonti: [sprint](sprint-v0.5.0.md), [decisioni](../decisions.md),
[gate](../milestones.json). Il candidato resta di sviluppo con modifiche
cumulative: freeze release e hosted CI non sono attestati da questa bozza.
