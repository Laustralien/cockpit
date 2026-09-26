<script lang="ts">
  /**
   * Creer un ticket qui m'est assigne. Les types se chargent quand la cle de projet est
   * validee (sortie du champ), pas a chaque frappe : « C », « CC » ne sont pas des projets.
   */
  import { onMount } from "svelte";
  import Modal from "../ui/Modal.svelte";
  import { trad } from "../../i18n";
  import { notify } from "../../stores/toast";
  import { jiraTypesTicket, jiraCreerTicket, jiraDemarrer, type LiaisonJira, type TypeTicketJira } from "../../api/jira";

  let { liaisons, cleParDefaut, onClose, onCree }: {
    liaisons: LiaisonJira[];
    cleParDefaut: string;
    onClose: () => void;
    onCree: (cle: string) => void;
  } = $props();

  let cleProjet = $state("");
  let types: TypeTicketJira[] = $state([]);
  let typeId = $state("");
  let resume = $state("");
  let description = $state("");
  let demarrerAussi = $state(false);
  let occupe = $state(false);

  const clesConnues = $derived([...new Set(liaisons.flatMap((l) => l.cles))]);
  const cleNormalisee = $derived(cleProjet.trim().toUpperCase());
  const liaisonDuProjet = $derived(liaisons.find((l) => l.cles.includes(cleNormalisee)) ?? null);

  onMount(() => {
    cleProjet = cleParDefaut;
    if (cleProjet) void chargerTypes();
  });

  async function chargerTypes() {
    types = [];
    typeId = "";
    const k = cleNormalisee;
    if (!/^[A-Z][A-Z0-9_]*$/.test(k)) return;
    try {
      const t = await jiraTypesTicket(k);
      if (k !== cleNormalisee) return;
      types = t;
      typeId = types[0]?.id ?? "";
    } catch (e) {
      if (k !== cleNormalisee) return;
      notify(String(e));
    }
  }

  async function creer() {
    occupe = true;
    let cle: string;
    try {
      cle = await jiraCreerTicket(cleNormalisee, typeId, resume, description.trim() || null);
      notify($trad("jira.ticketCree", { cle }), "success");
    } catch (e) {
      notify(String(e));
      occupe = false;
      return;
    }
    // Le ticket existe : un echec du demarrage ne doit pas le faire oublier.
    if (demarrerAussi && liaisonDuProjet) {
      try {
        const d = await jiraDemarrer(liaisonDuProjet.projet, cle);
        notify($trad(d.creee ? "jira.brancheCreee" : "jira.brancheReprise", { branche: d.branche }), "success");
        if (d.erreur_transition) notify($trad("jira.transitionEchouee", { erreur: d.erreur_transition }));
      } catch (e) {
        notify(String(e));
      }
    }
    occupe = false;
    onCree(cle);
  }
</script>

<Modal title={$trad("jira.creation.titre")} width="32rem" {onClose}>
  <div class="formulaire">
    <label>
      <span class="field-label">{$trad("jira.creation.projet")}</span>
      <input class="input" list="cles-jira" bind:value={cleProjet} onchange={chargerTypes} />
      <datalist id="cles-jira">
        {#each clesConnues as c}<option value={c}></option>{/each}
      </datalist>
    </label>
    <label>
      <span class="field-label">{$trad("jira.creation.type")}</span>
      <select class="input" bind:value={typeId} disabled={types.length === 0}>
        {#each types as t (t.id)}<option value={t.id}>{t.nom}</option>{/each}
      </select>
    </label>
    <label>
      <span class="field-label">{$trad("jira.creation.resume")}</span>
      <input class="input" bind:value={resume} />
    </label>
    <label>
      <span class="field-label">{$trad("jira.creation.description")}</span>
      <textarea class="input" rows="5" bind:value={description}></textarea>
    </label>
    {#if liaisonDuProjet}
      <label class="case">
        <input type="checkbox" bind:checked={demarrerAussi} />
        {$trad("jira.creation.demarrer")}
      </label>
    {/if}
    <div class="actions">
      <button class="btn" onclick={onClose}>{$trad("common.cancel")}</button>
      <button class="btn primary" disabled={occupe || !typeId || !resume.trim()} onclick={creer}>
        {$trad("jira.creation.creer")}
      </button>
    </div>
  </div>
</Modal>

<style>
  .formulaire { display: flex; flex-direction: column; gap: 0.8rem; }
  label { display: flex; flex-direction: column; gap: 0.3rem; }
  .case { flex-direction: row; align-items: center; gap: 0.5rem; }
  .actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
