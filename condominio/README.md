# Gestione Condominio

Gestionale condominiale **nativo** scritto in Rust (interfaccia egui, database SQLite incorporato).
Nessun server, nessuna installazione: i dati restano sul computer dell'amministratore.
Ispirato per funzioni e flusso di lavoro a [kondomanager-free](https://github.com/vince844/kondomanager-free).

## Modello di lavoro (sul modello di Domustudio)

Si lavora **per esercizio**, senza partita doppia: preventivo → rate → incassi/pagamenti (movimenti di gestione) →
consuntivo → conguaglio.

| Area | Funzioni |
|---|---|
| Anagrafiche | Persone, unità, più titolari per unità con quote pro-quota, tabelle millesimali con verifica del totale, import da CSV (passaggio da altri gestionali) |
| Preventivo e riparti | Piano dei conti gerarchico, badge Art. 1130-bis, voce ripartita su **più tabelle** (es. 70% scala / 30% generale), **esclusioni e riduzioni per unità**, blocco delle voci ancorate a piani attivi |
| Piani rate | Ordinari, straordinari (Art. 1135) e integrativi; delibera obbligatoria; riparto che somma al centesimo |
| Consuntivo e bilanci | Preventivo/consuntivo sintetico, dettagliato o misto, scostamenti, riparto e **bilancio individuale** per condòmino, **conguaglio** (rate integrative a debito, Salvadanaio a credito), rendiconto stampabile |
| Incassi e Salvadanaio | Incasso dalla rata più vecchia o distribuito a mano, compensazione dei crediti solo su tua scelta, storno self-healing |
| Morosità | Rate scadute, **interessi di mora** al tasso legale (calcolo giorno per giorno), **solleciti** (e-mail, lettera, raccomandata/PEC con interruzione della prescrizione) e storico |
| Cassa e banca | Conti correnti, movimenti, giroconti; **import estratto conto CSV** con riconciliazione assistita (incassi, fatture, F24) |
| Fornitori e fatture | Import **fatture elettroniche XML/.p7m**, ritenute 4% e 20%, pagamento, versamento F24 raggruppato per codice tributo |
| Fisco | Riepilogo **Certificazione Unica**, prospetto **770**, esportazione CSV |
| Assemblee e verbali | Convocazione (preavviso Art. 66 disp. att.), ordine del giorno, presenze e deleghe, **quorum** costitutivo e deliberativo (Art. 1136 e 1120 c.c.), voti per millesimi e per teste, generazione automatica delle delibere, verbale |
| Agenda e scadenze | Rate, fatture, F24, prescrizioni, assemblee, adempimenti fiscali e promemoria in un solo scadenzario |
| Pratiche | Interventi dei fornitori (anche da segnalazione guasto), sinistri assicurativi, pratiche legali con cronologia |
| Comunicazioni | Bacheca, circolari, coda e-mail SMTP con registro degli invii, segnalazioni guasto con priorità e notifiche |
| Documenti | Archivio con filtri e ricerca full-text |
| Conformità | Controlli Art. 1130, 1130-bis, 1135 c.c.; prescrizione quinquennale |

Tutte le stampe (rendiconto, bilancio individuale, estratti conto, scadenzario, morosità, CU, verbali, solleciti)
si aprono nel browser pronte per «Stampa / Salva come PDF».

Alla prima apertura puoi caricare un **condominio di esempio** per esplorare ogni schermata.
Gli importi sono sempre interi in centesimi (mai `f64`) e ogni operazione è atomica. Un database creato con la
versione a partita doppia viene migrato automaticamente.

## Compilare

```bash
cargo run --release          # avvio
cargo test                   # test del nucleo contabile e legale
```

### Linux (AppImage)
```bash
./packaging/build-appimage.sh   # produce Gestione_Condominio-x86_64.AppImage
```
Richiede le librerie grafiche di sistema (X11 o Wayland, OpenGL), presenti in ogni desktop.

### Windows
```powershell
cargo build --release           # target\release\condominio.exe
```
Il workflow `.github/workflows/release.yml` (avvio manuale da *Actions → Build → Run workflow*) produce:

* `Gestione_Condominio_Setup.exe` – installer per l'utente corrente (senza diritti di amministratore);
* `Gestione_Condominio_Portatile.zip` – cartella autonoma: decomprimila ovunque (anche su chiavetta USB).
  La presenza di `portable.txt` accanto all'exe fa salvare i dati nella cartella `dati` vicina al programma;
* l'AppImage Linux.

## Dove sono i dati

* Linux: `~/.local/share/condominio/`
* Windows: `%APPDATA%\gestionale\condominio\data\`

Contengono `condominio.db` e la cartella `archivio/` con i documenti. Backup dal menu *Impostazioni e Backup*
(il backup del database non include i file dell'archivio: copia anche quella cartella).

## Limiti noti

* Applicazione monoutente su un solo computer. Non ci sono i servizi cloud di Domustudio (portale/app condòmini
  Multidomus, addebiti SDD, bonifici ai fornitori dal programma, postalizzazione, SMS/VoIP): le segnalazioni le registra
  l'amministratore e le comunicazioni passano da stampa ed e-mail SMTP.
* Il 770 e la CU sono prospetti di supporto: la trasmissione telematica resta a carico del professionista.
* I controlli di conformità sono un ausilio, non un parere professionale. Verifica con un commercialista
  scadenze, codici tributo e aliquote prima dell'uso reale.

## Navigazione (stile Domustudio)

- **Barra dei menu**: File, Preventivo, Consuntivo, Comunicazioni, Fisco, Strumenti, ?
- **Barra dei pulsanti**: Agenda, Condomìni, Unità, Tabelle, Conti, Rate, Movimenti, Attività, Archivio, Assemblee, Persone, Fornitori. Ogni pulsante apre una riga di viste (es. Movimenti → Gestione, Fatture, Versamento rate, Personali, Giroconti, Operazioni su fondi, Banca).
- **Barra di contesto**: condominio ed esercizio attivi (F2 per cambiarli), notifiche.
- **Barra di stato** in basso; F1 = guida rapida.
