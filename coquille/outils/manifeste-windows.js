// Recalcule l'empreinte de l'installateur Windows dans `latest.yml`, une fois signe.
//
// **SIGNER CHANGE LES OCTETS DU FICHIER, DONC SON EMPREINTE.** electron-builder ecrit
// `latest.yml` et le `.blockmap` sur l'installateur NON signe ; SignPath rend ensuite un
// fichier different. Laisse tel quel, le manifeste annonce une empreinte que personne ne
// telecharge, et la mise a jour echoue chez chaque utilisateur Windows apres le
// telechargement. Le `.blockmap` (telechargement de la seule difference) est refait avec le
// meme outil qu'electron-builder, et c'est lui qui donne l'empreinte et la taille.
//
// Usage : node outils/manifeste-windows.js <dossier paquet>

const fs = require('node:fs')
const path = require('node:path')
const { buildBlockMap } = require('app-builder-lib/out/targets/blockmap/blockmap')

async function principal() {
  const dossier = process.argv[2]
  if (!dossier) throw new Error('usage : manifeste-windows.js <dossier paquet>')
  const manifeste = path.join(dossier, 'latest.yml')
  const texte = fs.readFileSync(manifeste, 'utf8')

  // Le manifeste nomme le fichier avec des tirets, le disque peut le porter avec des espaces
  // (voir l'etape de depot de la CI) : on compare les deux formes.
  const nomAnnonce = /^path:\s*(.+)$/m.exec(texte)?.[1]?.trim()
  if (!nomAnnonce) throw new Error(`${manifeste} ne nomme aucun installateur`)
  const installateur = fs
    .readdirSync(dossier)
    .find((nom) => nom.endsWith('.exe') && nom.replaceAll(' ', '-') === nomAnnonce)
  if (!installateur) throw new Error(`installateur ${nomAnnonce} absent de ${dossier}`)

  const fichier = path.join(dossier, installateur)
  const { sha512, size } = await buildBlockMap(fichier, 'gzip', `${fichier}.blockmap`)

  const anciennes = new Set([...texte.matchAll(/sha512:\s*(\S+)/g)].map((m) => m[1]))
  if (anciennes.size !== 1) {
    throw new Error(`${manifeste} porte ${anciennes.size} empreintes, une seule attendue`)
  }
  const nouveau = texte
    .replace(/(sha512:\s*)\S+/g, `$1${sha512}`)
    .replace(/(size:\s*)\d+/g, `$1${size}`)
  fs.writeFileSync(manifeste, nouveau)
  console.log(`${installateur} : ${size} octets, empreinte ${sha512.slice(0, 16)}...`)
}

principal().catch((e) => {
  console.error(`manifeste-windows : ${e.message}`)
  process.exit(1)
})
