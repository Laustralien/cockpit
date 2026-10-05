// Pose le nom du produit et sa version dans le backend Windows (`cockpit.exe`).
//
// **SIGNPATH REFUSE DE SIGNER UN BINAIRE QUI NE DIT PAS CE QU'IL EST** : chaque fichier signe
// doit porter le nom du projet et la version publiee. electron-builder le fait pour
// Cockpit.exe, pas pour le backend, que cargo produit sans aucune fiche. C'est aussi ce que
// Windows affiche dans les proprietes du fichier et dans le gestionnaire des taches, ou le
// service des terminaux tourne en permanence.
//
// On passe par l'outil d'electron-builder plutot que par un script de construction cargo :
// celui-ci exigerait un compilateur de ressources sur chaque poste, y compris pour la
// verification croisee depuis Linux.
//
// Usage : node outils/metadonnees-windows.js <chemin du .exe>

const path = require('node:path')
const { editWindowsResources } = require('app-builder-lib/out/util/resEdit')
const { version } = require('../package.json')

async function principal() {
  const fichier = process.argv[2]
  if (!fichier) throw new Error('usage : metadonnees-windows.js <chemin du .exe>')
  await editWindowsResources({
    file: fichier,
    fileVersion: version,
    productVersion: version,
    versionStrings: {
      ProductName: 'Cockpit',
      FileDescription: 'Cockpit (backend et service des terminaux)',
      InternalName: 'cockpit',
      OriginalFilename: path.basename(fichier),
      ProductVersion: version,
      LegalCopyright: 'MIT',
    },
  })
  console.log(`metadonnees posees sur ${fichier} : Cockpit ${version}`)
}

principal().catch((e) => {
  console.error(`metadonnees-windows : ${e.message}`)
  process.exit(1)
})
