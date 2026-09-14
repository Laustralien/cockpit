<script lang="ts">
  /// Ce qu'on a ecrit pour UN agent : ses consignes globales et ses competences.
  ///
  /// **ON EDITE LA CONFIGURATION D'UN AUTRE LOGICIEL, ET CA SE DIT.** Ce fichier est lu par le
  /// CLI, pas par Cockpit : son chemin complet est donc toujours affiche, et l'enregistrement
  /// est un geste explicite. Rien ne part sur le disque de quelqu'un sans qu'il sache ou.
  import { onMount } from "svelte";
  import { trad } from "../../i18n";
  import { notify } from "../../stores/toast";
  import { signalerErreur } from "../../stores/errors";
  import {
    llmConsignes,
    llmEcrireConsignes,
    llmCompetences,
    type CapacitesLlm,
    type EtatConsignes,
    type Competence,
  } from "../../api/llm";

  let { fournisseur, surRetour }: { fournisseur: CapacitesLlm; surRetour: () => void } = $props();

  let etat: EtatConsignes | null = $state(null);
  let competences: Competence[] = $state([]);
  let texte = $state("");
  /// Ce qui est RANGE sur le disque, pour savoir si l'on a des changements en cours.
  let texteRange = $state("");
  let chargement = $state(true);
  let enregistrement = $state(false);

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
</script>

<div class="detail">
  <div class="tete">
    <button class="btn" onclick={surRetour}>← {$trad("settings.ia.retourListe")}</button>
    <h3>
      <span class="symbole" style:color={fournisseur.couleur}>{fournisseur.symbole}</span>
      {fournisseur.nom}
    </h3>
  </div>

  {#if chargement}
    <p class="muted">{$trad("common.loading")}</p>
  {:else if !etat}
    <p class="muted">{$trad("settings.ia.consignesIndisponibles")}</p>
  {:else}
    <section class="card">
      <div class="card-head">
        <h3>{$trad("settings.ia.consignesTitre")}</h3>
        <p>{$trad("settings.ia.consignesSoustitre", { nom: fournisseur.nom })}</p>
      </div>

      <!-- Le chemin d'abord : on edite un fichier qui vit hors de Cockpit. -->
      <p class="chemin" title={etat.chemin}>
        {etat.chemin}
        {#if !etat.existe}
          <span class="badge off">{$trad("settings.ia.consignesAbsentes")}</span>
        {/if}
      </p>

      <textarea
        class="mono consignes"
        bind:value={texte}
        spellcheck="false"
        placeholder={$trad("settings.ia.consignesVide")}
      ></textarea>

      <div class="inline-row">
        <button class="btn primary" onclick={enregistrer} disabled={!modifie || enregistrement}>
          {$trad("common.save")}
        </button>
        <!-- Annuler ne se propose QUE s'il y a quelque chose a annuler : un bouton inerte
             invite au clic pour rien. -->
        {#if modifie}
          <button class="btn" onclick={() => (texte = texteRange)}>{$trad("common.cancel")}</button>
          <span class="muted">{$trad("settings.ia.consignesModifiees")}</span>
        {/if}
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <h3>{$trad("settings.ia.competencesTitre")}</h3>
        <p>{$trad("settings.ia.competencesSoustitre")}</p>
      </div>

      {#if competences.length === 0}
        <!-- Aucune competence n'est un etat normal, pas une panne : on le dit et on donne le
             dossier, pour que la premiere soit facile a poser. -->
        <p class="muted">{$trad("settings.ia.competencesAucune")}</p>
      {:else}
        <ul class="competences">
          {#each competences as c (c.chemin)}
            <li>
              <div class="c-nom">{c.nom}</div>
              {#if c.description}
                <div class="c-desc">{c.description}</div>
              {/if}
              <div class="c-chemin" title={c.chemin}>{c.chemin}</div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .detail { display: flex; flex-direction: column; gap: 16px; }
  .tete { display: flex; align-items: center; gap: 12px; }
  .tete h3 { margin: 0; display: flex; align-items: center; gap: 8px; font-size: 15px; }
  .symbole { font-size: 16px; }

  .chemin {
    margin: 0 0 10px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--text-muted);
    word-break: break-all;
  }

  .consignes {
    width: 100%;
    min-height: 320px;
    resize: vertical;
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm, 6px);
    /* Surface OPAQUE : sous image de fond, un `--bg-*` laisserait passer la photo derriere
       le texte qu'on est en train d'ecrire. */
    background: var(--surface-base, var(--bg-tertiary));
    color: var(--text-primary);
    font-size: 12px;
    line-height: 1.55;
    /* Le texte est long : on veut le lire comme il sera lu par l'agent. */
    white-space: pre-wrap;
  }

  .competences { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .competences li {
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm, 6px);
    background: var(--surface-base, var(--bg-tertiary));
  }
  .c-nom { font-weight: 600; font-size: 13px; color: var(--text-primary); }
  .c-desc {
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    /* Ces descriptions font parfois dix lignes : on en montre assez pour reconnaitre la
       competence, le reste se lit dans le fichier. */
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .c-chemin {
    margin-top: 6px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: var(--text-muted);
    word-break: break-all;
  }
  .muted { color: var(--text-muted); font-size: 12px; margin: 0; }
</style>
