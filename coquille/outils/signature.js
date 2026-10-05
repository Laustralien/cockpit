// Crochet de signature d'electron-builder, sous Windows seulement.
//
// **LE CONTROLE INTELLIGENT DES APPLICATIONS BLOQUE TOUT CE QUI N'EST PAS SIGNE**, et pas
// seulement l'installateur : Cockpit.exe, le backend, chaque DLL, et le programme de
// desinstallation. Un seul fichier non signe suffit pour que l'application entiere soit
// refusee, sans bouton « executer quand meme ».
//
// **LA SIGNATURE SE FAIT HORS D'ICI, CHEZ SIGNPATH, ET ELLE EST ASYNCHRONE** (une personne
// approuve chaque demande). Elle ne peut donc pas tourner pendant la construction. La CI
// construit en deux passes : la premiere produit l'application et le programme de
// desinstallation non signes, SignPath les signe, la seconde fabrique l'installateur autour
// des fichiers signes (`--prepackaged`), qui est signe a son tour.
//
// **LE PROGRAMME DE DESINSTALLATION NAIT PENDANT LA FABRICATION DE L'INSTALLATEUR**, puis y
// est enferme : SignPath ne sait pas l'en sortir, l'installateur NSIS n'etant pas un format
// qu'il sait ouvrir. electron-builder passe ce fichier par ce crochet juste avant de
// l'enfermer. Premiere passe : on le met de cote pour SignPath. Seconde passe : on le
// remplace par sa version signee. Les deux passes partagent configuration et version, donc
// le meme script NSIS : c'est le meme programme.
//
// Sans `COCKPIT_SIGNATURE_ETAPE`, le crochet ne fait rien : construction locale, ou CI sans
// SignPath configure.

const fs = require('node:fs')
const path = require('node:path')

const SUFFIXE_DESINSTALLATION = '__uninstaller.exe'

function dossierDeTravail() {
  const dossier = process.env.COCKPIT_SIGNATURE_DOSSIER
  if (!dossier) {
    throw new Error('COCKPIT_SIGNATURE_DOSSIER manque : ou ranger le programme de desinstallation ?')
  }
  return dossier
}

exports.default = async function signer(configuration) {
  const etape = process.env.COCKPIT_SIGNATURE_ETAPE
  if (!etape) return
  // electron-builder appelle le crochet une fois par algorithme d'empreinte (sha1 puis
  // sha256) : le second appel retrouverait le fichier deja traite.
  if (configuration.isNest) return
  if (!configuration.path.endsWith(SUFFIXE_DESINSTALLATION)) return

  const range = path.join(dossierDeTravail(), 'desinstallation.exe')
  if (etape === 'capturer') {
    fs.mkdirSync(path.dirname(range), { recursive: true })
    fs.copyFileSync(configuration.path, range)
    console.log(`signature : programme de desinstallation mis de cote -> ${range}`)
  } else if (etape === 'remplacer') {
    // Refuser plutot que d'enfermer un programme non signe : il serait bloque chez
    // l'utilisateur, qui ne pourrait plus desinstaller.
    if (!fs.existsSync(range)) {
      throw new Error(`programme de desinstallation signe introuvable : ${range}`)
    }
    fs.copyFileSync(range, configuration.path)
    console.log(`signature : programme de desinstallation signe pose -> ${configuration.path}`)
  } else {
    throw new Error(`COCKPIT_SIGNATURE_ETAPE inconnue : ${etape}`)
  }
}
