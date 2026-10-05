# Signature Windows (SignPath Foundation)

Windows 11 bloque tout programme non signé quand le Contrôle intelligent des applications est
actif, sans bouton pour passer outre. Un seul fichier non signé (installateur, `Cockpit.exe`,
backend, DLL, programme de désinstallation) suffit à faire refuser l'application entière.
Cockpit est signé gratuitement par le programme open source de SignPath Foundation.

## Ce que fait la CI (`release.yml`, job Windows)

1. `outils/metadonnees-windows.js` pose nom et version dans le backend `cockpit.exe`.
2. Passe 1 : electron-builder produit `win-unpacked/` et met de côté le programme de
   désinstallation (`outils/signature.js`, étape `capturer`).
3. Demande de signature n° 1 (configuration `application`) : tous les binaires.
4. Passe 2 : `--prepackaged` fabrique l'installateur autour des fichiers signés, avec le
   programme de désinstallation signé (étape `remplacer`).
5. Demande de signature n° 2 (configuration `installateur`).
6. `outils/manifeste-windows.js` recalcule l'empreinte de `latest.yml` et le `.blockmap` :
   sans ça, les mises à jour Windows échouent après le téléchargement.
7. Chaque binaire est vérifié (`Get-AuthenticodeSignature`) : un seul invalide, rien ne part.

Tant que les secrets manquent, Windows part non signé, avec un avertissement dans le journal.
**Chaque release demande deux approbations dans SignPath** ; le job attend jusqu'à deux heures
par demande.

## Mise en place, une fois

1. Demande depuis <https://signpath.org> (dépôt public, licence MIT, double authentification
   GitHub). Réponses utiles :
   - projet : Cockpit, `https://github.com/jguevel-tech/cockpit`, licence MIT ;
   - description : application de bureau qui regroupe terminaux persistants, notes, fichiers,
     Git et conteneurs, projet par projet (Electron + Rust) ;
   - téléchargement : page des releases GitHub ; politique de signature et confidentialité :
     sections *Code signing policy* et *Privacy* du README ;
   - construction : GitHub Actions, runners hébergés par GitHub, déclenchée par un tag `v*`.
2. Une fois le projet accepté, dans SignPath :
   - slug du projet : `cockpit` ; politique : `release-signing` ;
   - deux configurations d'artefact, `application` et `installateur` (ci-dessous) ;
   - lier le dépôt GitHub (connecteur GitHub Actions) ;
   - créer un jeton d'API pour un utilisateur CI.
3. Secrets GitHub du dépôt : `SIGNPATH_API_TOKEN` et `SIGNPATH_ORGANIZATION_ID`.

### Configuration `application`

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="desinstallation.exe">
      <authenticode-sign/>
    </pe-file>
    <directory path="win-unpacked">
      <pe-file path="Cockpit.exe">
        <authenticode-sign/>
      </pe-file>
      <pe-file-set>
        <include path="*.dll" min-matches="1" max-matches="unbounded"/>
        <for-each>
          <authenticode-sign/>
        </for-each>
      </pe-file-set>
      <directory path="resources">
        <pe-file path="cockpit.exe">
          <authenticode-sign/>
        </pe-file>
      </directory>
    </directory>
  </zip-file>
</artifact-configuration>
```

### Configuration `installateur`

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="Cockpit Setup *.exe">
      <authenticode-sign/>
    </pe-file>
  </zip-file>
</artifact-configuration>
```

## Après le premier build signé

Lire le sujet du certificat dans le journal de l'étape « Vérifier les signatures », puis le
poser dans `win.signtoolOptions.publisherName` : l'application refusera alors une mise à jour
qui ne porte pas cette signature. Ne pas le faire avant, sur un nom deviné : un nom faux bloque
toutes les mises à jour Windows.
