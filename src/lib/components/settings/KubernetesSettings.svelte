<script lang="ts">
  /**
   * Ce que Cockpit suit en continu, et pendant combien de temps.
   *
   * **RIEN N'EST SURVEILLE SANS QU'ON L'AIT DEMANDE.** Le cluster ne garde aucun historique :
   * pour voir une courbe sur une heure, il faut que quelqu'un ait mesure pendant cette heure.
   * Le faire d'office pour tous les projets serait un travail de fond que personne n'a demande,
   * et une question au cluster toutes les N secondes se paie multipliee par le nombre de
   * cibles. C'est donc l'utilisateur qui declare, un namespace a la fois.
   */
  import { onMount } from "svelte";
  import { notify } from "../../stores/toast";
  import { trad } from "../../i18n";
  import {
    k8sContextes, k8sNamespaces, k8sSurveillanceLire, k8sSurveillanceEcrire,
    type Cible, type Contexte, type ReglagesSurveillance,
  } from "../../api/k8s";
  import { grouperLesNamespaces, peutAllerA } from "../../k8s/vue";
  import { dureeCourte } from "../../k8s/mesures";

  let reglages: ReglagesSurveillance = $state({ cibles: [], retention_heures: 24 });
  let contextes: Contexte[] = $state([]);
  let chargement = $state(true);
  let panne: string | null = $state(null);

  /// L'ajout en cours.
  let ajout = $state(false);
  let contexteChoisi = $state("");
  let namespaces: string[] = $state([]);
  let chercheNamespace = $state("");
  let periodeChoisie = $state(60);

  /// Les rythmes proposes. Le plancher (15 s) est impose par le backend : en dessous, le
  /// serveur de mesures du cluster n'a rien de neuf a dire.
  const RYTHMES = [15, 30, 60, 300, 900];
  const RETENTIONS = [1, 6, 12, 24, 72, 168];

  const famillesDeNamespaces = $derived(
    grouperLesNamespaces(
      namespaces.filter((n) => n.toLowerCase().includes(chercheNamespace.toLowerCase())),
    ),
  );
  /// Ce que ca represente, en clair : une cible a 60 s sur 24 h, c'est 1 440 tours.
  const toursParJour = $derived(
    reglages.cibles
      .filter((c) => c.actif)
      .reduce((n, c) => n + Math.round(86400 / Math.max(15, c.periode)), 0),
  );

  onMount(() => void charger());

  async function charger() {
    chargement = true;
    try {
      reglages = await k8sSurveillanceLire();
      contextes = await k8sContextes();
    } catch (e) {
      panne = String(e);
    } finally {
      chargement = false;
    }
  }

  /// **LE BACKEND A LE DERNIER MOT.** Il borne le rythme et la retention, et rend ce qu'il a
  /// vraiment enregistre : on affiche CA, pas ce qu'on croyait envoyer.
  async function enregistrer(suite: ReglagesSurveillance) {
    try {
      reglages = await k8sSurveillanceEcrire(suite);
    } catch (e) {
      notify(String(e));
      await charger();
    }
  }

  async function choisirLeContexte(nom: string) {
    contexteChoisi = nom;
    namespaces = [];
    try {
      namespaces = await k8sNamespaces(nom);
    } catch (e) {
      // Un cluster qui refuse de lister ses namespaces n'empeche pas d'en saisir un.
      notify(String(e));
    }
  }

  function ajouter(namespace: string) {
    const deja = reglages.cibles.some(
      (c) => c.contexte === contexteChoisi && c.namespace === namespace,
    );
    if (deja) {
      notify($trad("k8s.dejaSurveille", { nom: namespace }));
      return;
    }
    const cible: Cible = {
      contexte: contexteChoisi,
      namespace,
      periode: periodeChoisie,
      actif: true,
    };
    void enregistrer({ ...reglages, cibles: [...reglages.cibles, cible] });
    ajout = false;
    chercheNamespace = "";
  }

  function modifier(index: number, champ: Partial<Cible>) {
    const cibles = reglages.cibles.map((c, i) => (i === index ? { ...c, ...champ } : c));
    void enregistrer({ ...reglages, cibles });
  }

  function retirer(index: number) {
    void enregistrer({
      ...reglages,
      cibles: reglages.cibles.filter((_, i) => i !== index),
    });
  }
</script>

<div class="k8s-reglages">
  <h3>{$trad("settings.k8s.titre")}</h3>
  <p class="intro">{$trad("settings.k8s.intro")}</p>

  {#if panne}
    <p class="erreur">{panne}</p>
  {:else if chargement}
    <p class="doux">{$trad("k8s.chargement")}</p>
  {:else}
    <section class="card">
      <div class="entete">
        <span class="titre">{$trad("settings.k8s.surveilles")}</span>
        <button class="btn small" onclick={() => (ajout = !ajout)}>
          {ajout ? $trad("k8s.annuler") : $trad("settings.k8s.ajouter")}
        </button>
      </div>

      {#if reglages.cibles.length === 0 && !ajout}
        <p class="doux">{$trad("settings.k8s.aucune")}</p>
      {/if}

      {#each reglages.cibles as cible, i (cible.contexte + cible.namespace)}
        <div class="cible" class:eteinte={!cible.actif}>
          <label class="inline">
            <input
              type="checkbox"
              checked={cible.actif}
              onchange={(e) => modifier(i, { actif: e.currentTarget.checked })}
            />
            <span class="nom">{cible.namespace}</span>
          </label>
          <span class="cluster">{cible.contexte}</span>
          <span class="espace"></span>
          <label class="inline petit-libelle">
            {$trad("k8s.rafraichissement")}
            <select
              class="input petit"
              value={cible.periode}
              onchange={(e) => modifier(i, { periode: Number(e.currentTarget.value) })}
            >
              {#each RYTHMES as r (r)}
                <option value={r}>{dureeCourte(r)}</option>
              {/each}
            </select>
          </label>
          <button class="btn small ghost" onclick={() => retirer(i)}>
            {$trad("settings.k8s.retirer")}
          </button>
        </div>
      {/each}

      {#if ajout}
        <div class="ajout">
          <div class="ligne-ajout">
            <label class="inline petit-libelle">
              {$trad("k8s.cluster")}
              <select
                class="input petit"
                value={contexteChoisi}
                onchange={(e) => void choisirLeContexte(e.currentTarget.value)}
              >
                <option value="">—</option>
                {#each contextes.filter((c) => !c.obstacle) as c (c.nom)}
                  <option value={c.nom}>{c.nom}</option>
                {/each}
              </select>
            </label>
            <label class="inline petit-libelle">
              {$trad("k8s.rafraichissement")}
              <select class="input petit" bind:value={periodeChoisie}>
                {#each RYTHMES as r (r)}
                  <option value={r}>{dureeCourte(r)}</option>
                {/each}
              </select>
            </label>
          </div>

          {#if contexteChoisi}
            <input
              class="input"
              bind:value={chercheNamespace}
              placeholder={$trad("k8s.chercherNamespace")}
            />
            {#if peutAllerA(chercheNamespace, namespaces)}
              <button class="btn small primary" onclick={() => ajouter(chercheNamespace.trim())}>
                {$trad("k8s.allerA", { nom: chercheNamespace.trim() })}
              </button>
            {/if}
            <div class="liste-namespaces">
              {#each famillesDeNamespaces as famille (famille.famille)}
                {#if famille.famille}<div class="famille">{famille.famille}</div>{/if}
                {#each famille.noms as n (n)}
                  <button class="entree" onclick={() => ajouter(n)}>{n}</button>
                {/each}
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </section>

    <section class="card">
      <div class="entete">
        <span class="titre">{$trad("settings.k8s.retention")}</span>
        <select
          class="input petit"
          value={reglages.retention_heures}
          onchange={(e) => void enregistrer({ ...reglages, retention_heures: Number(e.currentTarget.value) })}
        >
          {#each RETENTIONS as h (h)}
            <option value={h}>{dureeCourte(h * 3600)}</option>
          {/each}
        </select>
      </div>
      <p class="doux">{$trad("settings.k8s.retentionAide")}</p>
      {#if toursParJour > 0}
        <p class="doux">{$trad("settings.k8s.coutN", { n: toursParJour })}</p>
      {/if}
    </section>
  {/if}
</div>

<style>
  .k8s-reglages { display: flex; flex-direction: column; gap: 0.9rem; }
  h3 { margin: 0; font-size: 1rem; }
  .intro { margin: 0; color: var(--text-secondary); font-size: 0.85rem; line-height: 1.5; }
  .doux { margin: 0; color: var(--text-muted); font-size: 0.8rem; line-height: 1.5; }
  .erreur { color: var(--error); font-size: 0.85rem; }

  .entete { display: flex; align-items: center; justify-content: space-between; gap: 0.6rem; }
  .entete .titre { font-weight: 600; font-size: 0.88rem; }

  .cible {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.4rem 0;
    border-top: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .cible.eteinte .nom { color: var(--text-muted); text-decoration: line-through; }
  .cible .nom { font-family: var(--font-mono); font-size: 0.8rem; }
  .cluster { color: var(--text-muted); font-size: 0.74rem; }
  .espace { flex: 1; }
  .inline { display: flex; align-items: center; gap: 0.35rem; }
  .petit-libelle { color: var(--text-muted); font-size: 0.74rem; }
  .petit { width: auto; padding: 0.2rem 0.35rem; font-size: 0.78rem; }

  .ajout { display: flex; flex-direction: column; gap: 0.5rem; padding-top: 0.6rem; border-top: 1px solid var(--border); }
  .ligne-ajout { display: flex; gap: 0.8rem; flex-wrap: wrap; }
  .liste-namespaces { display: flex; flex-direction: column; gap: 0.08rem; max-height: 16rem; overflow: auto; }
  .famille {
    color: var(--text-muted);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0.5rem 0.3rem 0.1rem;
  }
  .entree {
    padding: 0.28rem 0.4rem;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    font-size: 0.82rem;
  }
  .entree:hover { background: var(--bg-tertiary); border-color: var(--border); }
</style>
