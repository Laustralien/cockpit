#!/bin/bash
# Banc d'interface : lance le paquet dans un ecran virtuel et capture des ecrans.
#
#   scripts/captures/banc-interface.sh <chemin/vers/l-AppImage> [dossier de sortie]
#
# **POURQUOI IL VIT DANS LE DEPOT.** Il a ete reecrit quatre fois dans `/tmp`, ou le systeme
# l'efface. Un banc qu'on rejoue a chaque correction d'interface merite d'etre range avec le
# reste, comme `prendre.sh` et `visite.sh` — dont il reprend le socle.
#
# Ce qu'il respecte, et qui n'est pas negociable (voir CLAUDE.md) :
#  - JAMAIS l'ecran reel : une fenetre de test y tomberait sur celle de l'utilisateur ;
#  - des dossiers XDG A LUI : un banc n'ecrit jamais dans le dossier de donnees de la machine ;
#  - un arret cible par une VARIABLE D'ENVIRONNEMENT posee sur ce lancement, jamais par le nom
#    du programme : le mainteneur fait tourner sa propre installation, qui porte le meme nom.
set -euo pipefail

# python-xlib et le reste vivent dans des prefixes a nous.
# Le fichier vit a cote du depot, avec les autres instructions de travail.
ENV_CONSTRUCTION="$(cd "$(dirname "$0")/../.." && pwd)/../.claude/env-construction.sh"
[ -f "$ENV_CONSTRUCTION" ] || { echo "env-construction.sh introuvable : $ENV_CONSTRUCTION" >&2; exit 1; }
# shellcheck disable=SC1090
source "$ENV_CONSTRUCTION"

BINAIRE="${1:?chemin de l AppImage attendu}"
TRAVAIL="${2:-/tmp/cockpit-banc-interface}"
ECRAN=:93
ICI="$(cd "$(dirname "$0")" && pwd)"
OUTILS="$ICI/outils.py"

for m in "$TRAVAIL"/run/gvfs "$TRAVAIL"/run/doc; do
  [ -d "$m" ] && { fusermount -u "$m" 2>/dev/null || fusermount -uz "$m" 2>/dev/null; } || true
done
rm -rf "$TRAVAIL"
mkdir -p "$TRAVAIL/run" "$TRAVAIL/home/projets" "$TRAVAIL/img" "$TRAVAIL/bin"
# **SANS CE FICHIER, ZSH LANCE SON ASSISTANT DE PREMIERE CONFIGURATION** et mange la premiere
# touche envoyee au terminal : la commande du banc partait amputee de sa premiere lettre.
touch "$TRAVAIL/home/.zshrc" 
chmod 700 "$TRAVAIL/run"

# **UN VRAI CLUSTER SE PRETE, IL NE SE COPIE PAS.** Pour eprouver l'ecran Kubernetes, on
# pointe le kubeconfig de la machine SANS le recopier dans le dossier du banc : un jeton
# d'acces a une production n'a rien a faire dans /tmp. Vide, l'ecran dit qu'il n'a pas de
# cluster, ce qui est aussi un cas a regarder.
export KUBECONFIG="${COCKPIT_BANC_KUBECONFIG:-}"
export XDG_RUNTIME_DIR="$TRAVAIL/run" XDG_DATA_HOME="$TRAVAIL/home/.local/share"
export XDG_CONFIG_HOME="$TRAVAIL/home/.config" XDG_CACHE_HOME="$TRAVAIL/home/.cache"
export HOME="$TRAVAIL/home" DISPLAY="$ECRAN" GIO_USE_VFS=local COCKPIT_LANGUE=fr
export COCKPIT_DB="$TRAVAIL/home/.local/share/com.cockpit.dev/cockpit.db"
export PATH="$TRAVAIL/bin:$PATH"
JETON="interface-$$"

arreter() { python3 "$OUTILS" arreter "COCKPIT_HARNAIS=$JETON" || true; }
trap 'arreter; kill %1 2>/dev/null || true' EXIT

# **UN FAUX AGENT, POUR EPROUVER CE QUI DEPEND D'UN AGENT.** Le service reconnait un agent au
# NOM du programme dans l'arbre de process : un script nomme `claude` suffit, et il evite de
# lancer le vrai (qui consommerait un abonnement et repondrait differemment a chaque fois).
# Il ecrit, puis se tait : c'est exactement « il attend une reponse ».
# **UN SCRIPT NE SUFFIT PAS : la detection lit `/proc/<pid>/exe`**, le vrai binaire, parce
# qu'argv[0] peut mentir (constate en 2026 sur un claude natif qui s'affichait sous un autre
# nom). Un `#!/bin/sh` donne donc `exe = /bin/sh` et passe inapercu. On COPIE donc un shell
# sous le nom `claude` : son chemin porte alors le nom que la detection cherche.
cp /bin/sh "$TRAVAIL/bin/claude"
cat > "$TRAVAIL/bin/faux-agent" <<'AGENT'
echo "je travaille"
sleep 2
echo "j ai fini d ecrire, j attends"
sleep 600
AGENT

Xvfb "$ECRAN" -screen 0 1680x1050x24 -nolisten tcp >/dev/null 2>&1 &
sleep 2
lancer() { COCKPIT_HARNAIS="$JETON" dbus-run-session -- "$BINAIRE" >>"$TRAVAIL/app.log" 2>&1 & sleep 20; }
clic()   { python3 "$OUTILS" cliquer "$1" "$2"; sleep "${3:-2}"; }
image()  { python3 "$OUTILS" capturer "$TRAVAIL/img/$1.png" >/dev/null; }

lancer; arreter; sleep 2
python3 "$OUTILS" preparer "$COCKPIT_DB" "$TRAVAIL/home/projets" fr
P="$TRAVAIL/home/projets/boutique-vinyles"
# `preparer` inscrit les projets en base mais ne cree pas leurs dossiers.
mkdir -p "$P"
git -C "$P" init -q -b main 2>/dev/null || true
git -C "$P" config user.email banc@exemple.test; git -C "$P" config user.name Banc
echo "demo" > "$P/README.md"; git -C "$P" add -A; git -C "$P" commit -qm "Poser les bases"

lancer
clic 105 296 3            # le projet « boutique-vinyles » (le premier de la liste)
clic 727 127 10           # l onglet Terminal
image 1-avant

if [ -n "${COCKPIT_BANC_K8S:-}" ]; then
  clic 941 127 14         # l onglet Kubernetes (a droite de Git)
  image k8s-1-liste
  clic 1000 228 1         # le champ de recherche
  python3 "$OUTILS" taper "auth" 2>/dev/null || true
  sleep 3
  image k8s-2-recherche
  clic 500 340 5          # le premier pod trouve : ouvre son detail
  image k8s-3-detail
  clic 1351 311 1         # fermer le detail
  clic 419 228 2          # le selecteur de cluster
  image k8s-4-clusters
  clic 402 355 2          # « + Ajouter un cluster » (sous la liste)
  image k8s-5-ajout
  echo "images kubernetes : $TRAVAIL/img"
  exit 0
fi

# On lance le faux agent dans le terminal affiche.
clic 900 500 1
python3 "$OUTILS" taper "claude $TRAVAIL/bin/faux-agent"
sleep 5
image 2-agent-en-cours
# **IL SE TAIT, MAIS ON LE REGARDE : LA BARRE LATERALE NE DOIT RIEN DIRE.** Un repere sur le
# terminal qu'on a sous les yeux s'effacait au clic puis revenait a la seconde suivante (0.74.0).
sleep 8
image 3-sous-les-yeux
# On part ailleurs : le repere doit apparaitre, c'est toute son utilite.
clic 797 127 4            # l onglet Fichiers
image 4-ailleurs
# Et il repart quand on revient, sans clignoter.
clic 713 127 4            # retour sur Terminal
image 5-de-retour

echo "images : $TRAVAIL/img"
grep -icE "error|erreur" "$TRAVAIL/app.log" | sed 's/^/  lignes d erreur dans le log : /'
