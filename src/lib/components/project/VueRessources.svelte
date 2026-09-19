<script lang="ts">
  /**
   * Les ressources du namespace : deux courbes, et les pods du plus gourmand au moins gourmand.
   *
   * **CE QU'ON NE SAIT PAS MESURER N'EST PAS DESSINE.** Le stockage et le reseau vivent dans un
   * Prometheus, que Cockpit n'interroge pas : deux courbes plates a leur place laisseraient
   * croire qu'il ne se passe rien. On le DIT, une fois, en bas de l'ecran.
   *
   * **L'HISTORIQUE COMMENCE A L'OUVERTURE**, parce que le cluster ne rend qu'un instantane.
   * L'ecran l'annonce plutot que de laisser croire a un service qui vient de demarrer.
   */
  import { trad } from "../../i18n";
  import Courbe from "./Courbe.svelte";
  import { formaterCpu, formaterRam, type Pod } from "../../k8s/vue";
  import type { Cible } from "../../api/k8s";
  import {
    couverture, dureeCourte, fenetre, lesPlusGourmands, periodesOffertes,
    RAFRAICHISSEMENTS, TOTAL, type Historique, type Mesure,
  } from "../../k8s/mesures";

  interface Props {
    pods: Pod[];
    historique: Historique;
    fenetreSecondes: number;
    rafraichissement: number;
    surFenetre: (secondes: number) => void;
    surRafraichissement: (secondes: number) => void;
    /// Le pod suivi de pres, ou `null` pour tout le namespace.
    focus: string | null;
    surFocus: (nom: string | null) => void;
    /// La cible affichee, si elle est surveillee en continu.
    surveillee: Cible | null;
    retentionHeures: number;
    surSurveillance: (actif: boolean, periode: number) => void;
  }
  let {
    pods, historique, fenetreSecondes, rafraichissement,
    surFenetre, surRafraichissement, focus, surFocus,
    surveillee, retentionHeures, surSurveillance,
  }: Props = $props();

  /// Le rythme propose quand on active depuis cet ecran. Le meme choix que dans les reglages.
  let periodeSurveillance = $state(60);
  const RYTHMES = [15, 30, 60, 300, 900];

  let trierSur: "cpu" | "ram" = $state("cpu");
  let maintenant = $state(Date.now());
  $effect(() => {
    const minuteur = setInterval(() => (maintenant = Date.now()), 1000);
    return () => clearInterval(minuteur);
  });

  /// Depuis combien de temps cet ecran mesure : c'est tout ce qu'on peut montrer.
  const couvert = $derived(couverture(historique.get(TOTAL) ?? [], maintenant));
  /// **LA VALEUR CHOISIE FIGURE TOUJOURS DANS LA LISTE.** Un `select` dont la valeur ne
  /// correspond a aucune option s'affiche VIDE : vu a l'ecran, avec une periode retenue de
  /// quinze minutes alors que l'ecran ne mesurait que depuis vingt secondes.
  const offertes = $derived(
    [...new Set([...periodesOffertes(couvert), fenetreSecondes])].sort((a, b) => a - b),
  );
  /// Une periode plus longue que ce qu'on a se ramene a ce qu'on a : on ne dessine pas du vide.
  const utile = $derived(Math.min(fenetreSecondes, Math.max(60, couvert)));
  const depuis = $derived(maintenant - utile * 1000);
  const serie = $derived(fenetre(historique.get(focus ?? TOTAL), depuis));
  /// Les mesures du dernier tour, pour le classement.
  const mesures = $derived<Mesure[]>(
    pods
      .filter((p) => p.cpu !== null || p.ram !== null)
      .map((p) => ({ nom: p.nom, cpu: p.cpu ?? 0, ram: p.ram ?? 0 })),
  );
  /// **UN POD TERMINE N'A PAS DE MESURE, ET CE N'EST PAS UNE MESURE A ZERO.** Releve du
  /// 2026-09-18 sur un namespace reel : 286 pods, dont 264 termines. Le serveur de mesures n'en
  /// connait que 22. Sans ce compte affiche, on cherche les 264 autres dans une liste ou ils ne
  /// seront jamais.
  const classement = $derived(lesPlusGourmands(mesures, trierSur, 60));
  const plusGros = $derived(Math.max(1, ...classement.map((m) => m[trierSur])));
  const suivi = $derived(focus ? pods.find((p) => p.nom === focus) : undefined);
</script>

<div class="ressources">
  <div class="reglages">
    {#if focus}
      <button class="fil" onclick={() => surFocus(null)}>
        ← {$trad("k8s.toutLeNamespace")}
      </button>
      <span class="focus">{focus}</span>
    {:else}
      <span class="focus">{$trad("k8s.toutLeNamespace")}</span>
    {/if}

    <span class="espace"></span>

    <label class="choix" title={$trad("k8s.fenetreAide")}>
      {$trad("k8s.fenetre")}
      <select class="input petit" value={fenetreSecondes} onchange={(e) => surFenetre(Number(e.currentTarget.value))}>
        {#each offertes as f (f)}
          <option value={f}>{dureeCourte(f)}</option>
        {/each}
      </select>
      <span class="couvert">{$trad("k8s.depuisOuvertureN", { duree: dureeCourte(couvert) })}</span>
    </label>

    <!-- **DEUX REGLAGES DE RYTHME A L'ECRAN, DONC DEUX NOMS DIFFERENTS.** Les deux s'appelaient
         « Rafraîchir » : l'un dit a quelle cadence CET ECRAN interroge le cluster, l'autre a
         quelle cadence l'enregistrement de fond ecrit en base. Le mainteneur ne savait pas
         lequel faisait quoi, et les valeurs affichees differaient. -->
    <label class="choix" title={$trad("k8s.mesureEnDirectAide")}>
      {$trad("k8s.mesureEnDirect")}
      <select class="input petit" value={rafraichissement} onchange={(e) => surRafraichissement(Number(e.currentTarget.value))}>
        {#each RAFRAICHISSEMENTS as r (r)}
          <option value={r}>{dureeCourte(r)}</option>
        {/each}
      </select>
    </label>
  </div>

  <!-- **DESACTIVE PAR DEFAUT, ET ON DIT POURQUOI.** Sans surveillance declaree, l'ecran ne
       connait que ce qu'il mesure pendant qu'on le regarde : une periode longue resterait vide,
       et on croirait a une panne. Le bouton ici ecrit le MEME reglage que Parametres ->
       Kubernetes, il n'y a pas deux verites. -->
  {#if surveillee}
    <div class="bandeau actif">
      <span class="pastille"></span>
      <span class="bandeau-texte">
        <strong>{$trad("k8s.enregistrementActif")}</strong>
        {$trad("k8s.enregistrementActifAide", { duree: dureeCourte(retentionHeures * 3600) })}
      </span>
      <span class="espace"></span>
      <span class="bandeau-actions">
        <label class="choix" title={$trad("k8s.uneMesureToutesLesAide")}>
          {$trad("k8s.uneMesureToutesLes")}
          <select
            class="input petit"
            value={surveillee.periode}
            onchange={(e) => surSurveillance(true, Number(e.currentTarget.value))}
          >
            {#each RYTHMES as r (r)}
              <option value={r}>{dureeCourte(r)}</option>
            {/each}
          </select>
        </label>
        <button class="btn small ghost" onclick={() => surSurveillance(false, 0)}>
          {$trad("k8s.arreterLEnregistrement")}
        </button>
      </span>
    </div>
  {:else}
    <div class="bandeau">
      <span class="bandeau-texte">
        <strong>{$trad("k8s.enregistrementEteint")}</strong>
        {$trad("k8s.enregistrementEteintAide")}
      </span>
      <span class="espace"></span>
      <span class="bandeau-actions">
        <label class="choix" title={$trad("k8s.uneMesureToutesLesAide")}>
          {$trad("k8s.uneMesureToutesLes")}
          <select class="input petit" bind:value={periodeSurveillance}>
            {#each RYTHMES as r (r)}
              <option value={r}>{dureeCourte(r)}</option>
            {/each}
          </select>
        </label>
        <button class="btn small primary" onclick={() => surSurveillance(true, periodeSurveillance)}>
          {$trad("k8s.activerLEnregistrement")}
        </button>
      </span>
    </div>
  {/if}

  <div class="courbes">
    <!-- **ON N'EMPILE QUE QUAND ON REGARDE TOUT LE NAMESPACE.** Un pod suivi de pres n'a rien
         a decomposer : sa propre courbe se lit mieux qu'une bande unique. -->
    <Courbe
      serie={serie}
      parPod={focus ? undefined : historique}
      valeur="cpu"
      titre={$trad("k8s.cpu")}
      formater={(n) => formaterCpu(Math.round(n))}
      teinte="accent"
      {depuis}
      jusqua={maintenant}
    />
    <Courbe
      serie={serie}
      parPod={focus ? undefined : historique}
      valeur="ram"
      titre={$trad("k8s.ram")}
      formater={(n) => formaterRam(Math.round(n))}
      teinte="succes"
      {depuis}
      jusqua={maintenant}
    />
  </div>

  {#if suivi}
    <div class="faits">
      <span>{suivi.etat}</span>
      <span>{suivi.groupe}</span>
      {#if suivi.machine}<span>⌗ {suivi.machine}</span>{/if}
      {#if suivi.redemarrages > 0}<span class="alerte">⟳ {suivi.redemarrages}</span>{/if}
    </div>
  {/if}

  <div class="classement">
    <div class="classement-tete">
      <span class="titre">{$trad("k8s.lesPlusGourmands")}</span>
      <span class="compte-mesures" title={$trad("k8s.mesuresCouvertureAide")}>
        {$trad("k8s.mesuresCouvertureN", { mesures: mesures.length, total: pods.length })}
      </span>
      <span class="bascules">
        <button class="bascule" class:actif={trierSur === "cpu"} onclick={() => (trierSur = "cpu")}>
          {$trad("k8s.cpu")}
        </button>
        <button class="bascule" class:actif={trierSur === "ram"} onclick={() => (trierSur = "ram")}>
          {$trad("k8s.ram")}
        </button>
      </span>
    </div>

    {#if classement.length === 0}
      <p class="vide">{$trad("k8s.sansMesures")}</p>
    {/if}

    {#each classement as m (m.nom)}
      <button class="rang" class:choisi={focus === m.nom} onclick={() => surFocus(focus === m.nom ? null : m.nom)}>
        <span class="rang-nom">{m.nom}</span>
        <span class="rang-jauge">
          <span
            class="rang-part {trierSur}"
            style="width:{Math.round((m[trierSur] / plusGros) * 100)}%"
          ></span>
        </span>
        <!-- **LES DEUX MESURES, TOUJOURS.** Un pod a « 0m » de processeur qui tient 127 Mo ne
             consomme pas rien : n'afficher que la mesure du tri le faisait croire, et c'est ce
             qui a fait douter des chiffres entiers. -->
        <span class="rang-valeur">{trierSur === "cpu" ? formaterCpu(m.cpu) : formaterRam(m.ram)}</span>
        <span class="rang-autre">{trierSur === "cpu" ? formaterRam(m.ram) : formaterCpu(m.cpu)}</span>
      </button>
    {/each}
  </div>

  <p class="note">{$trad("k8s.mesuresDepuisOuverture")}</p>
</div>

<style>
  .ressources {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow: auto;
    padding-right: 0.2rem;
    /* **L'UNIQUE ENFANT D'UN CONTENEUR FLEX NE S'ETEND PAS TOUT SEUL.** Sans cette ligne, la
       vue prenait la largeur de son contenu et laissait un tiers de l'ecran vide a droite,
       avec des courbes deux fois trop etroites. */
    flex: 1;
    min-width: 0;
  }
  .ressources > * { flex: none; }

  .bandeau {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    padding: 0.45rem 0.6rem;
    border: 1px dashed var(--border-color);
    border-radius: var(--radius);
    color: var(--text-secondary);
    font-size: 0.78rem;
  }
  .bandeau.actif { border-style: solid; border-color: var(--success); }
  /* Le nom de l'etat se lit d'abord, l'explication ensuite : sans cette difference, les deux
     phrases se valent et on relit la ligne entiere pour savoir si c'est allume ou eteint. */
  /* Le texte cede la place en se repliant ; les contrôles restent ensemble, sinon le bouton
     part seul sur une deuxieme ligne et on ne le rattache plus a ce qu'il commande. */
  .bandeau-texte { flex: 1 1 18rem; min-width: 0; }
  .bandeau-actions { display: flex; align-items: center; gap: 0.6rem; flex: none; }
  .bandeau-texte strong { color: var(--text-primary); font-weight: 600; margin-right: 0.3rem; }
  .bandeau .pastille {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--success);
    flex: none;
  }

  .reglages { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .espace { flex: 1; }
  /* Colle au titre, et pousse les bascules a droite : le compte parle du titre, pas du tri. */
  .compte-mesures {
    color: var(--text-muted);
    font-size: 0.72rem;
    margin: 0 auto 0 0.5rem;
  }
  /* La mesure qui ne sert pas au tri se lit en retrait : elle informe, elle ne se compare pas. */
  .rang-autre {
    color: var(--text-muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    min-width: 4.4rem;
    text-align: right;
  }
  .focus { color: var(--text-primary); font-weight: 600; font-size: 0.9rem; }
  .fil {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0;
  }
  .fil:hover { text-decoration: underline; }
  .couvert { color: var(--text-muted); font-size: 0.72rem; }
  .choix { display: flex; align-items: center; gap: 0.35rem; color: var(--text-muted); font-size: 0.76rem; }
  .petit { width: auto; padding: 0.22rem 0.4rem; font-size: 0.78rem; }

  .courbes { display: grid; grid-template-columns: 1fr 1fr; gap: 0.8rem; }
  @media (max-width: 900px) {
    .courbes { grid-template-columns: 1fr; }
  }

  .faits {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    color: var(--text-muted);
    font-size: 0.76rem;
  }
  .faits .alerte { color: var(--warning); }

  .classement {
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 0.5rem;
    background: var(--bg-secondary);
  }
  .classement-tete {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0 0.15rem 0.35rem;
  }
  .classement-tete .titre { color: var(--text-secondary); font-size: 0.78rem; }
  .bascules { display: flex; gap: 0.2rem; }
  .bascule {
    padding: 0.12rem 0.55rem;
    background: none;
    border: 1px solid var(--border-color);
    border-radius: 999px;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 0.72rem;
  }
  .bascule.actif { color: var(--text-primary); border-color: var(--accent); background: var(--accent-soft); }

  .rang {
    display: grid;
    grid-template-columns: minmax(8rem, 22rem) 1fr 4.2rem 4.4rem;
    align-items: center;
    gap: 0.6rem;
    width: 100%;
    padding: 0.28rem 0.3rem;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .rang:hover { background: var(--bg-tertiary); }
  .rang.choisi { border-color: var(--accent); background: var(--accent-soft); }
  .rang-nom {
    font-family: var(--font-mono);
    font-size: 0.74rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rang-jauge {
    height: 7px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    overflow: hidden;
  }
  .rang-part { display: block; height: 100%; border-radius: 999px; }
  .rang-part.cpu { background: var(--accent); }
  .rang-part.ram { background: var(--success); }
  .rang-valeur {
    color: var(--text-secondary);
    font-size: 0.74rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .vide, .note { color: var(--text-muted); font-size: 0.76rem; margin: 0; }
  .vide { text-align: center; padding: 1rem 0; }
</style>
