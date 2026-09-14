<script lang="ts">
  /// Ce qu'on a ecrit pour UN agent : ses consignes globales et ses competences.
  ///
  /// **ON EDITE LA CONFIGURATION D'UN AUTRE LOGICIEL, ET CA SE DIT.** Ce fichier est lu par le
  /// CLI, pas par Cockpit : son chemin complet est donc toujours affiche, et l'enregistrement
  /// est un geste explicite. Rien ne part sur le disque de quelqu'un sans qu'il sache ou.
  ///
  /// **CE COMPOSANT PORTE SON PROPRE STYLE, ET C'EST UNE LECON PAYEE.** Premiere version :
  /// elle reutilisait `.card` et `.inline-row` de l'ecran parent. Or `.inline-row` n'existe
  /// que dans le bloc de style de CE parent, et un style Svelte ne s'applique qu'au balisage
  /// de son propre fichier : tout arrivait colle, sans marge ni fond. C'est la regle du projet,
  /// (et ne pas ecrire ici le nom d'une balise entre chevrons : le compilateur la prend pour
  /// une vraie balise, meme dans un commentaire, et le fichier ne compile plus).
  /// « une classe portee par un composant enfant vit dans components.css » — ou, comme ici,
  /// chez l'enfant.
  import { onMount } from "svelte";
  import { trad } from "../../i18n";
  import { notify } from "../../stores/toast";
  import { signalerErreur } from "../../stores/errors";
  import AgentsView from "../agents/AgentsView.svelte";
  import {
    llmConsignes,
    llmEcrireConsignes,
    llmCompetences,
    llmOuvrirDossier,
    llmLireCompetence,
    type CapacitesLlm,
    type EtatConsignes,
    type Competence,
  } from "../../api/llm";

  let { fournisseur, surRetour }: { fournisseur: CapacitesLlm; surRetour: () => void } = $props();

  let etat: EtatConsignes | null = $state(null);
  let competences: Competence[] = $state([]);
  let texte = $state("");
  /// Ce qui est RANGE sur le disque, pour savoir s'il y a des changements en cours.
  let texteRange = $state("");
  let chargement = $state(true);
  let enregistrement = $state(false);
  /// La competence dont on lit la fiche, et son contenu.
  let ouverte: string | null = $state(null);
  let fiche = $state("");

  const modifie = $derived(etat !== null && texte !== texteRange);

  onMount(() => void charger());

  async function charger() {
    chargement = true;
    try {
      const [c, s] = await Promise.all([
        llmConsignes(fournisseur.id),
        llmCompetences(fournisseur.id),
      ]);
      etat = c;
      texte = c.contenu;
      texteRange = c.contenu;
      competences = s;
    } catch (e) {
      signalerErreur("llm.consignes", String(e));
    } finally {
      chargement = false;
    }
  }

  async function enregistrer() {
    if (!etat) return;
    enregistrement = true;
    try {
      await llmEcrireConsignes(fournisseur.id, texte);
      texteRange = texte;
      etat = { ...etat, existe: true };
      notify($trad("settings.ia.consignesEnregistrees"), "success");
    } catch (e) {
      notify(String(e));
    } finally {
      enregistrement = false;
    }
  }

  async function ouvrirLeDossier(competence?: string) {
    try {
      await llmOuvrirDossier(fournisseur.id, competence);
    } catch (e) {
      notify(String(e));
    }
  }

  /// Deplie la fiche d'une competence, ou la referme si c'est celle qui est ouverte.
  async function basculerLaFiche(nom: string) {
    if (ouverte === nom) {
      ouverte = null;
      return;
    }
    ouverte = nom;
    fiche = "";
    try {
      fiche = await llmLireCompetence(fournisseur.id, nom);
    } catch (e) {
      ouverte = null;
      notify(String(e));
    }
  }

  /// Combien de lignes fait le fichier : dit d'un coup d'oeil s'il est nourri ou vide.
  const lignes = $derived(texte.length === 0 ? 0 : texte.split("\n").length);
</script>

<div class="detail">
  <header class="tete">
    <button class="btn" onclick={surRetour}>← {$trad("settings.ia.retourListe")}</button>
    <h2>
      <span class="symbole" style:color={fournisseur.couleur}>{fournisseur.symbole}</span>
      {fournisseur.nom}
    </h2>
  </header>

  {#if chargement}
    <p class="muted">{$trad("common.loading")}</p>
  {:else if !etat}
    <p class="muted">{$trad("settings.ia.consignesIndisponibles")}</p>
  {:else}
    <section class="bloc">
      <div class="bloc-tete">
        <div>
          <h3>{$trad("settings.ia.consignesTitre")}</h3>
          <p class="soustitre">{$trad("settings.ia.consignesSoustitre", { nom: fournisseur.nom })}</p>
        </div>
        <button class="btn" onclick={() => ouvrirLeDossier()}>
          {$trad("settings.ia.ouvrirDossier")}
        </button>
      </div>

      <!-- Le chemin d'abord : on edite un fichier qui vit hors de Cockpit. -->
      <div class="fichier">
        <code title={etat.chemin}>{etat.chemin}</code>
        {#if !etat.existe}
          <span class="badge off">{$trad("settings.ia.consignesAbsentes")}</span>
        {:else}
          <span class="muted petit">{$trad("settings.ia.consignesLignes", { n: lignes })}</span>
        {/if}
      </div>

      <textarea
        class="consignes"
        bind:value={texte}
        spellcheck="false"
        placeholder={$trad("settings.ia.consignesVide")}
      ></textarea>

      <!-- La barre d'actions est SEPAREE de l'editeur : collee dessous, elle se lisait comme
           un bas de zone de texte, et le bouton passait inapercu. -->
      <footer class="actions">
        {#if modifie}
          <span class="muted petit">{$trad("settings.ia.consignesModifiees")}</span>
          <button class="btn" onclick={() => (texte = texteRange)}>{$trad("common.cancel")}</button>
        {/if}
        <button class="btn primary" onclick={enregistrer} disabled={!modifie || enregistrement}>
          {$trad("common.save")}
        </button>
      </footer>
    </section>

    <section class="bloc">
      <div class="bloc-tete">
        <div>
          <h3>{$trad("settings.ia.competencesTitre")} <span class="compte">{competences.length}</span></h3>
          <p class="soustitre">{$trad("settings.ia.competencesSoustitre")}</p>
        </div>
        {#if fournisseur.consignes}
          <button class="btn" onclick={() => ouvrirLeDossier()}>
            {$trad("settings.ia.ouvrirDossier")}
          </button>
        {/if}
      </div>

      {#if competences.length === 0}
        <p class="muted">{$trad("settings.ia.competencesAucune")}</p>
      {:else}
        <ul class="competences">
          {#each competences as c (c.chemin)}
            <li class="competence" class:ouverte={ouverte === c.nom}>
              <div class="c-ligne">
                <button class="c-nom" onclick={() => basculerLaFiche(c.nom)}>
                  <span class="chevron" aria-hidden="true">{ouverte === c.nom ? "▾" : "▸"}</span>
                  {c.nom}
                </button>
                <button class="btn petit-btn" onclick={() => ouvrirLeDossier(c.nom)}>
                  {$trad("settings.ia.ouvrirDossier")}
                </button>
              </div>
              {#if c.description}
                <p class="c-desc">{c.description}</p>
              {/if}
              {#if ouverte === c.nom}
                <pre class="c-fiche">{fiche || $trad("common.loading")}</pre>
                <div class="c-chemin" title={c.chemin}>{c.chemin}</div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if fournisseur.plugins}
      <!-- La bibliotheque d'agents vit DANS le detail du fournisseur qui la porte : elle
           ecrit dans la configuration de ce logiciel-la. Encastree par l'ecran parent, elle
           arrivait collee sous les cartes et sans leur cadre. -->
      <section class="bloc">
        <div class="bloc-tete">
          <div>
            <h3>{$trad("settings.ia.bibliothequeTitre")}</h3>
            <p class="soustitre">{$trad("settings.ia.bibliothequeSoustitre", { nom: fournisseur.nom })}</p>
          </div>
        </div>
        <div class="bibliotheque"><AgentsView /></div>
      </section>
    {/if}
  {/if}
</div>

<style>
  /* **TOUT LE STYLE VIT ICI.** Voir l'en-tete du script : reutiliser les classes du parent
     rendait cet ecran sans marges ni fond. */
  .detail {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .tete {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .tete h2 {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 1.05rem;
  }
  .symbole { font-size: 1.05rem; }

  .bloc {
    padding: 1.1rem 1.2rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }
  .bloc-tete {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 0.9rem;
  }
  .bloc-tete h3 {
    margin: 0 0 0.25rem;
    font-size: 1rem;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .compte {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }
  .soustitre {
    margin: 0;
    font-size: 0.8rem;
    color: var(--text-muted);
    line-height: 1.45;
    max-width: 70ch;
  }

  .fichier {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 0.6rem;
  }
  .fichier code {
    font-size: 0.72rem;
    color: var(--text-secondary);
    word-break: break-all;
  }

  .consignes {
    display: block;
    width: 100%;
    min-height: 340px;
    resize: vertical;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--border-color);
    border-radius: var(--radius, 8px);
    /* Surface OPAQUE : sous image de fond, un `--bg-*` laisserait passer la photo derriere
       le texte qu'on est en train d'ecrire. */
    background: var(--surface-base, var(--bg-tertiary));
    color: var(--text-primary);
    font-family: var(--font-mono, monospace);
    font-size: 0.78rem;
    line-height: 1.6;
  }
  .consignes:focus {
    outline: none;
    border-color: var(--accent);
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 0.85rem;
    padding-top: 0.85rem;
    border-top: 1px solid var(--border-color);
  }

  .competences {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .competence {
    padding: 0.7rem 0.85rem;
    border: 1px solid var(--border-color);
    border-radius: var(--radius, 8px);
    background: var(--surface-base, var(--bg-tertiary));
  }
  .competence.ouverte { border-color: var(--accent); }
  .c-ligne {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  /* Le nom EST le bouton qui deplie : une ligne cliquable sans bouton ne se voit pas. */
  .c-nom {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 600;
  }
  .c-nom:hover { color: var(--accent); }
  .chevron { color: var(--text-muted); font-size: 0.7rem; }
  .petit-btn { font-size: 0.72rem; padding: 2px 9px; }

  .c-desc {
    margin: 0.45rem 0 0;
    font-size: 0.78rem;
    color: var(--text-secondary);
    line-height: 1.5;
    /* Ces descriptions font parfois dix lignes : on en montre assez pour reconnaitre la
       competence, le reste se lit en depliant. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .competence.ouverte .c-desc {
    -webkit-line-clamp: unset;
    line-clamp: unset;
  }

  .c-fiche {
    margin: 0.7rem 0 0;
    padding: 0.7rem 0.8rem;
    max-height: 320px;
    overflow: auto;
    border-radius: var(--radius-sm, 6px);
    background: var(--bg-primary);
    color: var(--text-secondary);
    font-family: var(--font-mono, monospace);
    font-size: 0.72rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .c-chemin {
    margin-top: 0.5rem;
    font-family: var(--font-mono, monospace);
    font-size: 0.68rem;
    color: var(--text-muted);
    word-break: break-all;
  }

  /* **AgentsView EST CONCU POUR UNE VUE ENTIERE** (`height: 100%`). Encastre, son parent n'a
     pas de hauteur imposee : sans celle-ci, il s'ecrase a zero et la bibliotheque disparait.
     La contrainte vivait chez l'ecran parent ; elle suit le composant. */
  .bibliotheque {
    height: calc(100vh - var(--header-height) - 12rem);
    min-height: 26rem;
  }

  .muted { color: var(--text-muted); font-size: 0.8rem; margin: 0; }
  .petit { font-size: 0.72rem; }
</style>
