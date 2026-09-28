<script lang="ts">
  /**
   * Connexion a Jira Server / Data Center. Le jeton saisi part au backend et n'en revient
   * jamais : le champ reste vide une fois le jeton pose, et le laisser vide le garde.
   */
  import { onMount } from "svelte";
  import { notify } from "../../stores/toast";
  import { trad } from "../../i18n";
  import { jiraConfig, jiraPoserConfig, jiraTester, type ConfigJira } from "../../api/jira";

  let url = $state("");
  let jeton = $state("");
  let jetonPose = $state(false);
  let types: { jira: string; branche: string }[] = $state([]);
  let repli = $state("feature");
  let enregistrement = $state(false);
  let connecte: string | null = $state(null);

  onMount(async () => {
    try {
      appliquer(await jiraConfig());
    } catch (e) {
      notify(String(e));
    }
  });

  function appliquer(c: ConfigJira) {
    url = c.url;
    jetonPose = c.jeton_pose;
    jeton = "";
    repli = c.types_branche["*"] ?? "feature";
    types = Object.entries(c.types_branche)
      .filter(([k]) => k !== "*")
      .map(([jira, branche]) => ({ jira, branche }));
  }

  function correspondance(): Record<string, string> {
    const c: Record<string, string> = { "*": repli.trim() || "feature" };
    for (const t of types) if (t.jira.trim() && t.branche.trim()) c[t.jira.trim()] = t.branche.trim();
    return c;
  }

  async function enregistrer() {
    enregistrement = true;
    try {
      appliquer(await jiraPoserConfig(url, jeton.trim() || null, correspondance()));
      notify($trad("jira.reglages.enregistre"), "success");
    } catch (e) {
      notify(String(e));
    } finally {
      enregistrement = false;
    }
  }

  async function tester() {
    connecte = null;
    try {
      connecte = $trad("jira.reglages.connecteEn", { nom: await jiraTester() });
    } catch (e) {
      notify(String(e));
    }
  }
</script>

<div class="stack">
  <section class="card">
    <div class="card-head">
      <h3>{$trad("jira.reglages.titre")}</h3>
      <p>{$trad("jira.reglages.intro")}</p>
    </div>
    <label class="champ">
      <span class="field-label">{$trad("jira.reglages.url")}</span>
      <input class="input" bind:value={url} placeholder={$trad("jira.reglages.urlExemple")} />
    </label>
    <label class="champ">
      <span class="field-label">{$trad("jira.reglages.jeton")}</span>
      <input
        class="input"
        type="password"
        autocomplete="off"
        bind:value={jeton}
        placeholder={jetonPose ? $trad("jira.reglages.jetonPose") : ""}
      />
    </label>
  </section>

  <section class="card">
    <div class="card-head">
      <h3>{$trad("jira.reglages.types")}</h3>
      <p>{$trad("jira.reglages.typesIntro")}</p>
    </div>
    {#each types as t, i}
      <div class="ligne">
        <input class="input" bind:value={t.jira} placeholder={$trad("jira.reglages.typeJiraExemple")} />
        <span>→</span>
        <input class="input" bind:value={t.branche} placeholder={$trad("jira.reglages.typeBrancheExemple")} />
        <button class="icon-btn danger" onclick={() => types.splice(i, 1)} aria-label={$trad("common.delete")}>✕</button>
      </div>
    {/each}
    <div class="ligne">
      <span class="repli">{$trad("jira.reglages.autresTypes")}</span>
      <span>→</span>
      <input class="input" bind:value={repli} />
      <span class="place"></span>
    </div>
    <button class="btn small" onclick={() => types.push({ jira: "", branche: "" })}>
      {$trad("jira.reglages.ajouterType")}
    </button>
  </section>

  <div class="actions">
    <button class="btn primary" onclick={enregistrer} disabled={enregistrement}>{$trad("common.save")}</button>
    <button class="btn" onclick={tester} disabled={!jetonPose}>{$trad("jira.reglages.tester")}</button>
    {#if connecte}<span class="ok">✓ {connecte}</span>{/if}
  </div>
</div>

<style>
  .champ { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.8rem; }
  .ligne { display: grid; grid-template-columns: 1fr auto 1fr 26px; gap: 0.5rem; align-items: center; margin-bottom: 0.5rem; }
  .repli { color: var(--text-secondary); font-size: 0.87rem; }
  .actions { display: flex; gap: 0.6rem; align-items: center; }
  .ok { color: var(--success, var(--text-secondary)); font-size: 0.87rem; }
</style>
