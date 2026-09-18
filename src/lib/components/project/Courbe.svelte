<script lang="ts" module>
  /// **UN DEGRADE SVG SE DESIGNE PAR SON ID, ET DEUX COURBES NE PEUVENT PAS PARTAGER LE MEME** :
  /// la seconde peindrait avec le dégradé de la première. Un compteur de module suffit, et il
  /// ne depend d'aucune prop.
  let compteur = 0;
</script>

<script lang="ts">
  /**
   * Une courbe : aire, ligne, grille, et la valeur sous le curseur.
   *
   * **LE TRACE SE FAIT EN PIXELS REELS, PAS EN `viewBox` ETIREE.** Un SVG etire deforme les
   * traits — une ligne de 1,5 px devient un trait gras d'un cote et un cheveu de l'autre. On
   * mesure donc la largeur disponible et on dessine dedans.
   */
  import {
    aire, borneHaute, graduations, graduationsDeTemps, heureDe, leplusProche, ligne, points,
    type Point,
  } from "../../k8s/mesures";

  interface Props {
    serie: Point[];
    valeur: "cpu" | "ram";
    titre: string;
    /** Met une valeur brute en texte lisible (millicores, octets…). */
    formater: (n: number) => string;
    /** `accent` pour le processeur, `succes` pour la memoire. */
    teinte?: "accent" | "succes";
    depuis: number;
    jusqua: number;
    hauteur?: number;
  }
  let {
    serie, valeur, titre, formater, teinte = "accent", depuis, jusqua, hauteur = 132,
  }: Props = $props();

  let largeur = $state(0);
  let survol: { x: number; point: Point } | null = $state(null);

  const cadre = $derived({ largeur: Math.max(1, largeur), hauteur });
  const max = $derived(borneHaute(serie.map((p) => p[valeur])));
  const coords = $derived(points(serie, valeur, cadre, depuis, jusqua, max));
  const trace = $derived(ligne(coords));
  const dessous = $derived(aire(coords, cadre));
  const derniere = $derived(serie.length > 0 ? serie[serie.length - 1][valeur] : null);
  const identifiant = `courbe-${(compteur += 1)}`;
  /// **LES HEURES SE LISENT SOUS LA COURBE, SANS PROMENER LA SOURIS.** « Ca a grimpe vers
  /// 14 h 30 » est ce qu'on vient chercher ; l'infobulle ne le donnait qu'un point a la fois.
  const heures = $derived(graduationsDeTemps(depuis, jusqua, cadre.largeur));

  function surLaSouris(e: MouseEvent) {
    const boite = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const x = e.clientX - boite.left;
    const point = leplusProche(serie, x, cadre, depuis, jusqua);
    survol = point ? { x, point } : null;
  }
</script>

<div class="courbe {teinte}">
  <div class="tete">
    <span class="titre">{titre}</span>
    {#if derniere !== null}
      <span class="derniere">{formater(derniere)}</span>
    {/if}
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="cadre"
    style="height:{hauteur}px"
    bind:clientWidth={largeur}
    onmousemove={surLaSouris}
    onmouseleave={() => (survol = null)}
  >
    <svg width={cadre.largeur} height={hauteur} aria-hidden="true">
      <defs>
        <linearGradient id={identifiant} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" class="haut" />
          <stop offset="100%" class="bas" />
        </linearGradient>
      </defs>

      {#each graduations(max) as g (g)}
        {@const y = hauteur - (g / max) * hauteur}
        <line class="grille" x1="0" y1={y} x2={cadre.largeur} y2={y} />
      {/each}

      {#each heures as h (h.t)}
        <line class="grille verticale" x1={h.x} y1="0" x2={h.x} y2={hauteur} />
      {/each}

      {#if dessous}
        <path class="aire" d={dessous} fill="url(#{identifiant})" />
        <path class="trait" d={trace} />
      {/if}

      {#if survol}
        {@const x = ((survol.point.t - depuis) / Math.max(1, jusqua - depuis)) * cadre.largeur}
        {@const y = hauteur - (Math.min(survol.point[valeur], max) / max) * hauteur}
        <line class="repere" x1={x} y1="0" x2={x} y2={hauteur} />
        <circle class="point" cx={x} cy={y} r="3" />
      {/if}
    </svg>

    <div class="echelle">
      {#each [...graduations(max)].reverse() as g (g)}
        <span>{formater(g)}</span>
      {/each}
    </div>

    {#if survol}
      <div
        class="infobulle"
        style="left:{Math.min(Math.max(survol.x, 54), cadre.largeur - 54)}px"
      >
        <strong>{formater(survol.point[valeur])}</strong>
        <span>{heureDe(survol.point.t)}</span>
      </div>
    {/if}

    {#if serie.length === 0}
      <p class="attente">—</p>
    {/if}
  </div>

  <!-- Sous le cadre, pas dedans : pose sur la courbe, un libelle devient illisible des que le
       trace passe derriere lui. -->
  <div class="heures" style="height:{serie.length === 0 ? 0 : 14}px">
    {#each heures as h (h.t)}
      <!-- Le dernier repere tombe souvent sur le bord droit : centre, il serait coupe en deux. -->
      {@const bord = h.x > cadre.largeur - 26 ? "droite" : h.x < 26 ? "gauche" : ""}
      <span class={bord} style="left:{h.x}px">{h.libelle}</span>
    {/each}
  </div>
</div>

<style>
  .courbe { display: flex; flex-direction: column; gap: 0.3rem; min-width: 0; }
  .tete { display: flex; align-items: baseline; justify-content: space-between; gap: 0.5rem; }
  .titre { color: var(--text-secondary); font-size: 0.78rem; }
  .derniere {
    color: var(--text-primary);
    font-size: 1.05rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .cadre {
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
    overflow: hidden;
  }
  svg { display: block; }

  .grille { stroke: var(--border); stroke-width: 1; stroke-dasharray: 2 4; opacity: 0.7; }
  /* Plus discrete que l'horizontale : elle sert de repere, elle ne quadrille pas le fond. */
  .grille.verticale { opacity: 0.4; }
  .aire { stroke: none; }
  .trait { fill: none; stroke-width: 1.6; stroke-linejoin: round; stroke-linecap: round; }
  .repere { stroke: var(--text-muted); stroke-width: 1; stroke-dasharray: 2 3; }
  .point { stroke: var(--bg-secondary); stroke-width: 1.5; }

  .accent .trait { stroke: var(--accent); }
  .accent .point { fill: var(--accent); }
  .accent .haut { stop-color: var(--accent); stop-opacity: 0.32; }
  .accent .bas { stop-color: var(--accent); stop-opacity: 0.02; }
  .succes .trait { stroke: var(--success); }
  .succes .point { fill: var(--success); }
  .succes .haut { stop-color: var(--success); stop-opacity: 0.3; }
  .succes .bas { stop-color: var(--success); stop-opacity: 0.02; }

  .echelle {
    position: absolute;
    inset: 0 auto 0 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 0.1rem 0.35rem;
    color: var(--text-muted);
    font-size: 0.66rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  /* **UNE HEURE NE SE COUPE PAS EN DEUX AU BORD DU CADRE.** Le libelle est centre sur son
     repere, sauf aux extremites ou il se cale a l'interieur. */
  .heures {
    position: relative;
    color: var(--text-muted);
    font-size: 0.64rem;
    font-variant-numeric: tabular-nums;
    overflow: hidden;
  }
  .heures span {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    white-space: nowrap;
  }
  .heures span.gauche { transform: none; }
  .heures span.droite { transform: translateX(-100%); }

  .infobulle {
    position: absolute;
    top: 0.35rem;
    transform: translateX(-50%);
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    background: var(--surface-1);
    border: 1px solid var(--border-strong);
    font-size: 0.72rem;
    white-space: nowrap;
    pointer-events: none;
  }
  .infobulle span { color: var(--text-muted); }

  .attente {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: var(--text-muted);
  }
</style>
