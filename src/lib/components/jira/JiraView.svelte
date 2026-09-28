<script lang="ts">
  /**
   * Mes tickets Jira. `projet` absent : vue globale avec filtre ; present : filtre fixe
   * (onglet d'un projet).
   */
  import { onMount } from "svelte";
  import { trad } from "../../i18n";
  import { notify } from "../../stores/toast";
  import { activeView } from "../../stores/ui";
  import { jiraConfig, jiraLiaisons, jiraMesTickets, type LiaisonJira, type TicketJira } from "../../api/jira";
  import { grouperParCategorie, liaisonDuTicket, clesDuFiltre } from "../../jira/tickets";
  import TicketDetail from "./TicketDetail.svelte";
  import NouveauTicket from "./NouveauTicket.svelte";

  let { projet = null }: { projet?: string | null } = $props();

  let configure: boolean | null = $state(null);
  let liaisons: LiaisonJira[] = $state([]);
  let filtre = $state("");
  let tickets: TicketJira[] = $state([]);
  let chargement = $state(false);
  let ouvert: string | null = $state(null);
  let creation = $state(false);

  const groupes = $derived(grouperParCategorie(tickets));
  const ticketOuvert = $derived(tickets.find((t) => t.cle === ouvert) ?? null);
  const cleParDefaut = $derived(clesDuFiltre(filtre, liaisons)?.[0] ?? "");

  onMount(async () => {
    filtre = projet ?? "";
    try {
      const c = await jiraConfig();
      configure = !!c.url && c.jeton_pose;
      if (!configure) return;
      liaisons = await jiraLiaisons();
      await rafraichir();
    } catch (e) {
      notify(String(e));
    }
  });

  async function rafraichir() {
    chargement = true;
    try {
      tickets = await jiraMesTickets(clesDuFiltre(filtre, liaisons));
    } catch (e) {
      notify(String(e));
    } finally {
      chargement = false;
    }
  }
</script>

{#snippet groupe(titre: string, liste: TicketJira[])}
  <section class="card">
    <div class="card-head"><h3>{titre} <span class="badge">{liste.length}</span></h3></div>
    {#if liste.length === 0}
      <p class="empty">{$trad("jira.aucun")}</p>
    {:else}
      {#each liste as t (t.cle)}
        <button class="ligne" class:actif={ouvert === t.cle} onclick={() => (ouvert = t.cle)}>
          <span class="cle">{t.cle}</span>
          <span class="resume">{t.resume}</span>
          <span class="meta">{t.type_ticket}</span>
          <span class="meta">{t.priorite}</span>
          <span class="meta">{t.statut}</span>
          <span class="meta">{t.maj_le.slice(0, 10)}</span>
        </button>
      {/each}
    {/if}
  </section>
{/snippet}

<div class="jira">
  {#if configure === false}
    <section class="card">
      <p>{$trad("jira.nonConfigure")}</p>
      <button class="btn" onclick={() => activeView.set("settings")}>{$trad("jira.ouvrirReglages")}</button>
    </section>
  {:else if configure}
    <div class="barre">
      {#if !projet}
        <select class="input" bind:value={filtre} onchange={rafraichir}>
          <option value="">{$trad("jira.tousLesProjets")}</option>
          {#each liaisons as l (l.projet)}
            <option value={l.projet}>{l.projet} ({l.cles.join(", ")})</option>
          {/each}
        </select>
      {/if}
      <button class="btn" onclick={rafraichir} disabled={chargement}>{$trad("common.refresh")}</button>
      <button class="btn primary" onclick={() => (creation = true)}>{$trad("jira.nouveau")}</button>
    </div>

    <div class="contenu" class:avec-detail={!!ouvert}>
      <div class="stack">
        {@render groupe($trad("jira.enCours"), groupes.enCours)}
        {@render groupe($trad("jira.aFaire"), groupes.aFaire)}
      </div>
      {#if ouvert}
        <TicketDetail
          cle={ouvert}
          liaison={ticketOuvert ? liaisonDuTicket(ticketOuvert, liaisons) : null}
          onClose={() => (ouvert = null)}
          onChange={rafraichir}
        />
      {/if}
    </div>
    {#if creation}
      <NouveauTicket
        {liaisons}
        {cleParDefaut}
        onClose={() => (creation = false)}
        onCree={(cle) => { creation = false; ouvert = cle; void rafraichir(); }}
      />
    {/if}
  {/if}
</div>

<style>
  .jira { flex: 1; min-width: 0; }
  .barre { display: flex; gap: 0.6rem; margin-bottom: 1rem; }
  .barre select { max-width: 20rem; }
  .contenu { display: grid; grid-template-columns: 1fr; gap: 1rem; align-items: start; }
  .contenu.avec-detail { grid-template-columns: 1fr minmax(20rem, 30rem); }
  .ligne {
    display: grid; grid-template-columns: 7rem 1fr 6rem 5rem 7rem 6rem; gap: 0.6rem;
    width: 100%; padding: 0.4rem 0.5rem; text-align: left; font: inherit; font-size: 0.85rem;
    background: none; border: none; border-radius: 4px; color: var(--text-primary); cursor: pointer;
  }
  .ligne:hover, .ligne.actif { background: var(--bg-tertiary); }
  .cle { font-weight: 600; }
  .resume { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
