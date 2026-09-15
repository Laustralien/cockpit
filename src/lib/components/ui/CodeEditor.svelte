<script lang="ts">
  // Editeur de code leger : textarea transparent superpose au rendu Shiki
  // (technique react-simple-code-editor). Les deux couches partagent police,
  // taille, line-height et padding — l'alignement doit rester EXACT.
  import { highlightCode } from "../../shiki";

  let {
    value = $bindable(""),
    lang,
    dark,
    onSave,
    curseurInitial,
  }: {
    value: string;
    lang: string;
    dark: boolean;
    onSave: () => void;
    /// Ou poser le curseur a l'ouverture, en nombre de caracteres depuis le debut.
    ///
    /// **C'EST CE QUI REND L'EDITION IMMEDIATE SUPPORTABLE** : on entre en edition en
    /// cliquant dans le texte, et le curseur doit tomber LA ou l'on a clique. Sans ca, il
    /// atterrit au debut du fichier et il faut retrouver sa ligne.
    curseurInitial?: number;
  } = $props();

  // La coloration est relancee apres une pause de frappe. Mesure dans le WebKitGTK
  // de Tauri : 37 ms sur 1500 lignes de markdown, 105 ms sur 1000 lignes de
  // TypeScript — la colorer a chaque touche couterait ce prix a chaque touche.
  const HIGHLIGHT_DEBOUNCE_MS = 120;

  let html = $state("");
  // La couche coloree est en retard sur le texte tape. Vrai au montage : la
  // premiere coloration n'est pas encore calculee.
  let stale = $state(true);
  let hlEl: HTMLDivElement | undefined = $state();
  let taEl: HTMLTextAreaElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    // Le \n final garde la derniere ligne rendue meme vide (alignement scroll)
    const v = value + "\n";
    const l = lang;
    const d = dark;
    stale = true;
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      const out = await highlightCode(v, l, d);
      // Une frappe plus recente a pris la main pendant le calcul : son propre
      // timer s'en occupe, ce resultat est perime.
      if (v !== value + "\n") return;
      html = out;
      stale = false;
      // Le {@html} remplace le DOM de la couche : on la recale sur le textarea.
      syncScroll();
    }, HIGHLIGHT_DEBOUNCE_MS);
  });

  // Le focus, et le curseur la ou l'on a clique. `$effect` sans dependance lue en dehors :
  // il ne s'execute qu'au montage, donc taper ne le rejoue pas.
  $effect(() => {
    if (!taEl) return;
    taEl.focus();
    if (curseurInitial !== undefined) {
      const ou = Math.max(0, Math.min(curseurInitial, taEl.value.length));
      taEl.setSelectionRange(ou, ou);
      // Le textarea ne fait pas defiler tout seul vers une selection posee par programme.
      // Sans ca, on clique en bas d'un fichier de mille lignes et l'editeur s'ouvre en haut.
      const lignesAvant = taEl.value.slice(0, ou).split("\n").length - 1;
      const hauteurLigne = parseFloat(getComputedStyle(taEl).lineHeight) || 18;
      taEl.scrollTop = Math.max(0, lignesAvant * hauteurLigne - taEl.clientHeight / 2);
      syncScroll();
    }
  });

  function syncScroll() {
    if (hlEl && taEl) {
      hlEl.scrollTop = taEl.scrollTop;
      hlEl.scrollLeft = taEl.scrollLeft;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      onSave();
    } else if (e.key === "Tab") {
      // execCommand preserve l'historique undo natif du textarea
      e.preventDefault();
      document.execCommand("insertText", false, "  ");
    }
  }
</script>

<div class="editor" class:stale>
  <div class="hl" bind:this={hlEl} aria-hidden="true">{@html html}</div>
  <textarea
    bind:this={taEl}
    bind:value
    spellcheck="false"
    wrap="off"
    onscroll={syncScroll}
    onkeydown={onKeydown}
  ></textarea>
</div>

<style>
  .editor { position: relative; height: 100%; min-height: 0; }
  /* Les DEUX couches : memes metriques de texte, sinon decalage visuel */
  .hl, textarea {
    font-family: var(--font-mono, monospace);
    font-size: 0.82rem;
    line-height: 1.5;
    tab-size: 4;
    white-space: pre;
    overflow: auto;
  }
  .hl {
    position: absolute; inset: 0;
    pointer-events: none;
    overflow: hidden;
  }
  .hl :global(pre) {
    margin: 0; padding: 0.8rem 1rem;
    background: transparent !important;
    font-family: inherit; font-size: inherit; line-height: inherit;
  }
  .hl :global(code) { font-family: inherit; font-size: inherit; line-height: inherit; }
  textarea {
    position: absolute; inset: 0;
    padding: 0.8rem 1rem;
    background: transparent;
    color: transparent;
    caret-color: var(--text-primary);
    border: none; outline: none; resize: none;
  }
  textarea::selection { background: var(--accent-soft); }
  /* Tant que la couche coloree est en retard, c'est le texte du textarea qui
     s'affiche. Sans ca on tape dans le vide : le textarea est transparent et la
     pause de 120 ms est remise a zero a chaque touche, donc une frappe continue
     ne repeignait JAMAIS (mesure au banc WebKitGTK : 0 caractere sur 33 visible
     a 80 ms/touche, quelle que soit la taille du fichier).
     Les deux couches ont les memes metriques : la substitution ne deplace rien. */
  .editor.stale .hl { visibility: hidden; }
  .editor.stale textarea { color: var(--text-primary); }
</style>
