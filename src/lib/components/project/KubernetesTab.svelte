<script lang="ts">
  /**
   * L'ecran des pods d'un namespace.
   *
   * **CE QUI FAIT LA DIFFERENCE AVEC L'ECRAN D'EN FACE : L'ORDRE ET LE FLUX.** Un namespace
   * reel contient 298 pods dont 23 qui tournent ; tout est affiche, mais ce qui reclame une
   * action est en haut et ce qui est fini en bas. Et la liste se met a jour par un FLUX : on
   * ne recharge jamais les 6,3 Mo que pese sa lecture complete.
   */
  import { onMount, onDestroy } from "svelte";
  import { ecouter, type Detacher } from "../../coquille";
  import { activeTab, pendingTerminalCommand } from "../../stores/ui";
  import { notify } from "../../stores/toast";
  import { signalerErreur } from "../../stores/errors";
  import { getAppSettings, setAppSetting } from "../../api/recorder";
  import { trad } from "../../i18n";
  import {
    k8sContextes, k8sNamespaces, k8sPods, k8sLogs, k8sEvenements, k8sYaml,
    k8sKubectlPresent, k8sAjouterUnCluster, type Contexte,
  } from "../../api/k8s";
  import { invoke } from "../../coquille";
  import {
    filtrer, grouper, compter, formaterCpu, formaterRam, age, grouperLesNamespaces,
    appliquer, appliquerLesMesures, type Pod, type Mesure,
  } from "../../k8s/vue";

  let { name }: { name: string } = $props();

  let contextes: Contexte[] = $state([]);
  let contexte = $state("");
  let namespaces: string[] = $state([]);
  let namespace = $state("");
  let pods: Pod[] = $state([]);
  let sansMesures: string | null = $state(null);
  let recherche = $state("");
  let chargement = $state(false);
  let panne: string | null = $state(null);
  let enDirect = $state(false);
  let replies = $state(new Set<string>());
  /// Vrai tant qu'on n'a pas encore range l'ecran pour ce namespace.
  let premiereLecture = $state(true);
  /// Le bouton « tout deplier » de la barre.
  let deplieTout = $state(false);
  let kubectl = $state(false);

  /// L'ajout d'un cluster : ce qu'on colle, et ce qui en est ressorti.
  let ajoutOuvert = $state(false);
  let texteColle = $state("");
  let ajoutEnCours = $state(false);

  /// Le choix ouvert : « cluster », « namespace », ou rien.
  let ouvert: "cluster" | "namespace" | null = $state(null);
  let chercheNamespace = $state("");

  /// Le pod dont on regarde le detail, et ce qu'on y regarde.
  let choisi: Pod | null = $state(null);
  let volet: "logs" | "evenements" | "yaml" = $state("logs");
  let contenu = $state("");
  let contenuCharge = $state(false);
  let conteneurChoisi: string | null = $state(null);
  let logsPrecedents = $state(false);

  /// L'age se recalcule a l'affichage : fige, il vieillirait sans bouger.
  let maintenant = $state(Date.now());
  let horloge: ReturnType<typeof setInterval> | null = null;
  let detacheurs: Detacher[] = [];

  const cle = $derived(`k8s.cible.${name}`);
  const visibles = $derived(filtrer(pods, recherche));
  const groupes = $derived(grouper(visibles));
  /// **CHERCHER, C'EST OUVRIR.** Un groupe replie repondrait « un resultat » sans montrer
  /// lequel, et il faudrait un clic de plus pour voir ce qu'on vient de demander.
  const toutOuvert = $derived(recherche.trim().length > 0 || deplieTout);
  const comptes = $derived(compter(pods));
  const contexteActif = $derived(contextes.find((c) => c.nom === contexte));
  const famillesDeNamespaces = $derived(
    grouperLesNamespaces(
      namespaces.filter((n) => n.toLowerCase().includes(chercheNamespace.toLowerCase())),
    ),
  );

  onMount(() => {
    horloge = setInterval(() => (maintenant = Date.now()), 1000);
    void demarrer();
    return () => {
      if (horloge) clearInterval(horloge);
    };
  });

  // **LE FLUX S'ARRETE EN PARTANT.** Sans ca, une connexion reste ouverte sur le cluster et
  // l'interface continue de traiter des changements pour un ecran que personne ne regarde.
  onDestroy(() => {
    detacher();
    void invoke("k8s_arreter_le_suivi").catch(() => {});
  });

  function detacher() {
    for (const d of detacheurs) d();
    detacheurs = [];
    enDirect = false;
  }

  async function demarrer() {
    try {
      contextes = await k8sContextes();
      kubectl = await k8sKubectlPresent();
    } catch (e) {
      panne = String(e);
      return;
    }
    const reglages = await getAppSettings().catch(() => ({}) as Record<string, string>);
    let vise: { contexte?: string; namespace?: string } = {};
    try {
      vise = JSON.parse(reglages[cle] ?? "{}");
    } catch {
      // Un reglage illisible ne doit pas empecher d'ouvrir l'ecran : on repart du defaut.
      vise = {};
    }
    const servables = contextes.filter((c) => !c.obstacle);
    const choisiParDefaut =
      servables.find((c) => c.nom === vise.contexte) ??
      servables.find((c) => c.courant) ??
      servables[0];
    if (!choisiParDefaut) return;
    await choisirLeCluster(choisiParDefaut.nom, vise.namespace ?? null);
  }

  async function choisirLeCluster(nom: string, namespaceVise: string | null = null) {
    contexte = nom;
    ouvert = null;
    pods = [];
    panne = null;
    detacher();
    chargement = true;
    try {
      namespaces = await k8sNamespaces(nom);
    } catch (e) {
      panne = String(e);
      chargement = false;
      return;
    }
    const duContexte = contextes.find((c) => c.nom === nom)?.namespace ?? null;
    const cible =
      (namespaceVise && namespaces.includes(namespaceVise) && namespaceVise) ||
      (duContexte && namespaces.includes(duContexte) && duContexte) ||
      namespaces[0];
    await choisirLeNamespace(cible);
  }

  async function choisirLeNamespace(nom: string) {
    namespace = nom;
    premiereLecture = true;
    ouvert = null;
    chercheNamespace = "";
    choisi = null;
    detacher();
    void setAppSetting(cle, JSON.stringify({ contexte, namespace })).catch((e) =>
      signalerErreur("k8s.reglage", String(e)),
    );
    await charger();
  }

  /// Lit la liste ENTIERE, puis passe le relais au flux. C'est la seule lecture complete.
  async function charger() {
    if (!contexte || !namespace) return;
    chargement = true;
    panne = null;
    try {
      const vue = await k8sPods(contexte, namespace);
      pods = vue.pods;
      sansMesures = vue.sans_mesures;
      // **L'ECRAN S'OUVRE SUR LES GROUPES, PAS SUR 294 LIGNES.** La tete d'un groupe porte
      // deja ce qu'on vient verifier : combien tournent sur combien, quelle version, et un
      // cadre rouge si quelque chose reclame une action. Rien n'est masque, chaque groupe
      // annonce son nombre de pods et s'ouvre d'un clic. Sans ca, quatorze pods d'un travail
      // mort repoussaient les services hors de l'ecran.
      // On ne le recalcule qu'au premier chargement du namespace : sinon le flux refermerait
      // sous les doigts ce que l'utilisateur vient d'ouvrir.
      if (premiereLecture) {
        replies = new Set(grouper(vue.pods).map((g) => g.sorte + g.nom));
        premiereLecture = false;
      }
      suivre(vue.version);
    } catch (e) {
      panne = String(e);
      pods = [];
    } finally {
      chargement = false;
    }
  }

  /// Branche le flux a partir de la version de la liste qu'on vient de lire : reprendre
  /// ailleurs laisserait passer en silence ce qui a change entre les deux.
  function suivre(version: string) {
    detacher();
    detacheurs = [
      ecouter<{ sorte: string; pod: Pod }>("k8s_changement", (e) => {
        pods = appliquer(pods, e.payload.sorte, e.payload.pod);
        if (choisi && e.payload.pod.nom === choisi.nom) choisi = e.payload.pod;
      }),
      ecouter<Mesure[]>("k8s_mesures", (e) => {
        pods = appliquerLesMesures(pods, e.payload);
      }),
      // Notre point de reprise n'est plus valable : le cluster nous le dit, on relit.
      ecouter<null>("k8s_relire", () => void charger()),
      ecouter<string>("k8s_panne", (e) => {
        enDirect = false;
        panne = e.payload;
      }),
    ];
    void invoke("k8s_suivre", { contexte, namespace, version })
      .then(() => (enDirect = true))
      .catch((e) => {
        enDirect = false;
        signalerErreur("k8s.suivi", String(e));
      });
  }

  /// Ajoute un cluster a partir du kubeconfig telecharge depuis son interface web.
  ///
  /// **ON ECRIT DANS LE FICHIER STANDARD DE LA MACHINE**, celui que lisent aussi kubectl et
  /// les autres outils : ce qu'on ajoute ici sert partout, et Cockpit ne detient rien a part.
  async function ajouterUnCluster() {
    const texte = texteColle.trim();
    if (!texte) return;
    ajoutEnCours = true;
    try {
      const quoi = await k8sAjouterUnCluster(texte);
      // **ON EFFACE CE QU'ON VIENT DE COLLER.** Un jeton d'acces n'a pas a rester affiche
      // dans un champ derriere une fenetre qu'on laisse ouverte.
      texteColle = "";
      ajoutOuvert = false;
      contextes = await k8sContextes();
      const messages: string[] = [];
      if (quoi.renommes.length > 0) messages.push($trad("k8s.ajoutRenomme", { noms: quoi.renommes.join(", ") }));
      if (quoi.renouveles.length > 0) messages.push($trad("k8s.ajoutRenouvele", { noms: quoi.renouveles.join(", ") }));
      if (quoi.deja_la.length > 0) messages.push($trad("k8s.ajoutDejaLa", { noms: quoi.deja_la.join(", ") }));
      if (messages.length === 0) messages.push($trad("k8s.ajoutFait", { noms: quoi.ajoutes.join(", ") }));
      notify(messages.join(" "));
      const neuf = quoi.ajoutes[0];
      if (neuf && quoi.deja_la.length === 0) await choisirLeCluster(neuf);
    } catch (e) {
      notify(String(e));
    } finally {
      ajoutEnCours = false;
    }
  }

  /// Un fichier depose vaut un collage : c'est le fichier que l'interface web fait telecharger.
  async function surDepot(e: DragEvent) {
    e.preventDefault();
    const fichier = e.dataTransfer?.files?.[0];
    if (!fichier) return;
    try {
      texteColle = await fichier.text();
    } catch (err) {
      notify(String(err));
    }
  }

  function basculer(nom: string) {
    const suite = new Set(replies);
    if (suite.has(nom)) suite.delete(nom);
    else suite.add(nom);
    replies = suite;
  }

  async function ouvrirLeDetail(pod: Pod, lequel: "logs" | "evenements" | "yaml" = "logs") {
    choisi = pod;
    volet = lequel;
    conteneurChoisi = pod.noms_conteneurs[0] ?? null;
    logsPrecedents = false;
    await lireLeDetail();
  }

  async function lireLeDetail() {
    if (!choisi) return;
    contenuCharge = false;
    contenu = "";
    try {
      if (volet === "logs") {
        contenu = await k8sLogs(contexte, namespace, choisi.nom, conteneurChoisi, 500, logsPrecedents);
        if (!contenu.trim()) contenu = $trad("k8s.logsVides");
      } else if (volet === "yaml") {
        contenu = await k8sYaml(contexte, namespace, choisi.nom);
      } else {
        const liste = await k8sEvenements(contexte, namespace, choisi.nom);
        contenu = liste.length === 0
          ? $trad("k8s.aucunEvenement")
          : liste.map(formaterEvenement).join("\n");
      }
    } catch (e) {
      contenu = String(e);
    } finally {
      contenuCharge = true;
    }
  }

  function formaterEvenement(e: Record<string, unknown>): string {
    const quand = String(e.lastTimestamp ?? e.eventTime ?? "");
    const heure = quand ? new Date(quand).toLocaleString() : "";
    const compte = Number(e.count ?? 1);
    const fois = compte > 1 ? ` (${$trad("k8s.foisN", { n: compte })})` : "";
    return `${heure}  ${e.type ?? ""}  ${e.reason ?? ""}${fois}\n    ${e.message ?? ""}`;
  }

  /// Ouvre un shell DANS le conteneur, par un vrai terminal Cockpit : on y retrouve ses
  /// volets, son historique et son copier-coller. Seul ce bouton demande `kubectl`.
  function ouvrirUnShell(pod: Pod) {
    const conteneur = pod.noms_conteneurs[0];
    const ou = conteneur ? ` -c ${conteneur}` : "";
    const commande =
      `kubectl --context ${contexte} -n ${namespace} exec -it ${pod.nom}${ou}` +
      ` -- sh -c '[ -x /bin/bash ] && exec bash || exec sh'`;
    pendingTerminalCommand.set({ project: name, command: commande });
    activeTab.set("terminal");
  }

  async function copier() {
    try {
      await navigator.clipboard.writeText(contenu);
      notify($trad("k8s.copie"));
    } catch (e) {
      notify(String(e));
    }
  }

  function couleurDe(pod: Pod): string {
    if (pod.ennuyeux) return "mauvais";
    if (pod.etat === "Succeeded") return "fini";
    if (pod.etat === "Running" && pod.prets === pod.conteneurs) return "bon";
    return "attente";
  }
</script>

<div class="k8s">
  <div class="barre">
    <div class="choix">
      <button
        class="selecteur"
        onclick={() => (ouvert = ouvert === "cluster" ? null : "cluster")}
        disabled={contextes.length === 0}
        title={contexteActif?.serveur ?? ""}
      >
        <span class="etiquette">{$trad("k8s.cluster")}</span>
        <span class="valeur">{contexte || $trad("k8s.aucun")}</span>
        <span class="fleche">▾</span>
      </button>

      <button
        class="selecteur"
        onclick={() => (ouvert = ouvert === "namespace" ? null : "namespace")}
        disabled={namespaces.length === 0}
      >
        <span class="etiquette">{$trad("k8s.namespace")}</span>
        <span class="valeur">{namespace || "—"}</span>
        <span class="fleche">▾</span>
      </button>
    </div>

    <input
      class="input recherche"
      type="search"
      bind:value={recherche}
      placeholder={$trad("k8s.chercher")}
      disabled={pods.length === 0}
    />

    <div class="comptes">
      {#if pods.length > 0}
        <span class="compte bon">{$trad("k8s.enMarcheN", { n: comptes.enMarche })}</span>
        {#if comptes.ennuyeux > 0}
          <span class="compte mauvais">{$trad("k8s.ennuyeuxN", { n: comptes.ennuyeux })}</span>
        {/if}
        {#if comptes.termines > 0}
          <span class="compte fini">{$trad("k8s.terminesN", { n: comptes.termines })}</span>
        {/if}
      {/if}
      <span class="direct" class:actif={enDirect} title={enDirect ? $trad("k8s.directAide") : ""}>
        {enDirect ? $trad("k8s.direct") : $trad("k8s.arrete")}
      </span>
      <button class="btn small ghost" onclick={() => (deplieTout = !deplieTout)} disabled={pods.length === 0}>
        {deplieTout ? $trad("k8s.replierTout") : $trad("k8s.deplierTout")}
      </button>
      <button class="btn small ghost" onclick={() => void charger()} disabled={chargement}>
        {$trad("k8s.relire")}
      </button>
    </div>
  </div>

  {#if ouvert === "cluster"}
    <div class="panneau">
      {#each contextes as c (c.nom)}
        <button
          class="entree"
          class:active={c.nom === contexte}
          disabled={!!c.obstacle}
          onclick={() => void choisirLeCluster(c.nom)}
          title={c.obstacle ?? c.serveur}
        >
          <span class="nom">{c.nom}</span>
          <span class="detail-entree">{c.obstacle ?? c.serveur}</span>
        </button>
      {/each}

      {#if !ajoutOuvert}
        <button class="btn small ajouter" onclick={() => (ajoutOuvert = true)}>
          {$trad("k8s.ajouter")}
        </button>
      {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="ajout" ondrop={surDepot} ondragover={(e) => e.preventDefault()}>
          <ol class="etapes">
            <li>{$trad("k8s.ajouterEtape1")}</li>
            <li>{$trad("k8s.ajouterEtape2")}</li>
            <li>{$trad("k8s.ajouterEtape3")}</li>
          </ol>
          <textarea
            class="input colle"
            bind:value={texteColle}
            placeholder={$trad("k8s.ajouterZone")}
            spellcheck="false"
          ></textarea>
          <div class="bas">
            <button class="btn small primary" onclick={() => void ajouterUnCluster()} disabled={ajoutEnCours || !texteColle.trim()}>
              {$trad("k8s.ajouterBouton")}
            </button>
            <button class="btn small ghost" onclick={() => { ajoutOuvert = false; texteColle = ""; }}>
              {$trad("k8s.annuler")}
            </button>
            <span class="note">{$trad("k8s.ajouterNote")}</span>
          </div>
        </div>
      {/if}
    </div>
  {:else if ouvert === "namespace"}
    <div class="panneau">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="input"
        bind:value={chercheNamespace}
        placeholder={$trad("k8s.chercherNamespace")}
        autofocus
      />
      <div class="liste-namespaces">
        {#each famillesDeNamespaces as famille (famille.famille)}
          {#if famille.famille}
            <div class="famille">{famille.famille}</div>
          {/if}
          {#each famille.noms as n (n)}
            <button class="entree" class:active={n === namespace} onclick={() => void choisirLeNamespace(n)}>
              <span class="nom">{n}</span>
            </button>
          {/each}
        {/each}
      </div>
    </div>
  {/if}

  <!-- **UNE PANNE PASSE AVANT « IL N'Y A RIEN ».** Dans l'autre ordre, un backend qui refuse
       de repondre affichait « aucun cluster » : l'ecran envoyait chercher un kubeconfig
       correct au lieu de dire ce qui s'etait passe. Meme famille que « un silence ne vaut que
       s'il ne peut couvrir qu'un seul cas ». -->
  {#if panne}
    <div class="empty">
      <p class="mauvais">{panne}</p>
      <button class="btn" onclick={() => void charger()}>{$trad("k8s.reessayer")}</button>
    </div>
  {:else if contextes.length === 0}
    <div class="empty">
      <p>{$trad("k8s.aucunKubeconfig")}</p>
      <p class="aide">{$trad("k8s.aucunKubeconfigAide")}</p>
    </div>
  {:else}
    <div class="corps">
      <div class="liste">
        {#if sansMesures}
          <div class="avis" title={sansMesures}>{$trad("k8s.sansMesures")}</div>
        {/if}
        {#if chargement && pods.length === 0}
          <div class="empty">{$trad("k8s.chargement")}</div>
        {:else if groupes.length === 0}
          <div class="empty">{recherche ? $trad("k8s.aucunResultat") : $trad("k8s.aucunPod")}</div>
        {/if}

        {#each groupes as g (g.sorte + g.nom)}
          {@const replie = !toutOuvert && replies.has(g.sorte + g.nom)}
          <section class="groupe" class:mauvais={g.ennuyeux}>
            <button class="tete" onclick={() => basculer(g.sorte + g.nom)}>
              <span class="fleche">{replie ? "▸" : "▾"}</span>
              <span class="nom">{g.nom}</span>
              <span class="sorte">{g.sorte}</span>
              <span class="prets">{g.prets}/{g.attendus}</span>
              {#each g.versions as v (v)}
                <span class="version">{v}</span>
              {/each}
              <span class="mesure">{formaterCpu(g.cpu)}</span>
              <span class="mesure">{formaterRam(g.ram)}</span>
              <span class="nombre">{$trad("k8s.podsN", { n: g.pods.length })}</span>
            </button>

            {#if !replie}
              {#each g.pods as p (p.nom)}
                <div class="pod" class:choisi={choisi?.nom === p.nom}>
                  <button class="ligne" onclick={() => void ouvrirLeDetail(p)}>
                    <span class="pastille {couleurDe(p)}"></span>
                    <span class="nom">{p.nom}</span>
                    <span class="etat">{p.etat}</span>
                    <span class="age">{age(p.depuis, maintenant)}</span>
                    {#if p.redemarrages > 0}
                      <span class="redemarrages" title={$trad("k8s.redemarrages")}>
                        ⟳ {p.redemarrages}
                      </span>
                    {/if}
                    <span class="mesure">{formaterCpu(p.cpu)}</span>
                    <span class="mesure">{formaterRam(p.ram)}</span>
                  </button>
                  <span class="actions">
                    <button class="btn small ghost" onclick={() => void ouvrirLeDetail(p, "logs")}>
                      {$trad("k8s.logs")}
                    </button>
                    {#if kubectl}
                      <button class="btn small ghost" onclick={() => ouvrirUnShell(p)}>
                        {$trad("k8s.shell")}
                      </button>
                    {/if}
                  </span>
                </div>
              {/each}
            {/if}
          </section>
        {/each}
      </div>

      {#if choisi}
        <aside class="detail">
          <div class="tete-detail">
            <span class="nom">{choisi.nom}</span>
            <button class="btn small ghost" onclick={() => (choisi = null)} aria-label={$trad("k8s.fermer")}>
              ✕
            </button>
          </div>
          <div class="onglets">
            {#each [["logs", $trad("k8s.logs")], ["evenements", $trad("k8s.evenements")], ["yaml", $trad("k8s.yaml")]] as [id, libelle] (id)}
              <button
                class="onglet"
                class:actif={volet === id}
                onclick={() => { volet = id as typeof volet; void lireLeDetail(); }}
              >{libelle}</button>
            {/each}
          </div>

          {#if volet === "logs"}
            <div class="options">
              {#if choisi.noms_conteneurs.length > 1}
                <select
                  class="input small"
                  bind:value={conteneurChoisi}
                  onchange={() => void lireLeDetail()}
                >
                  {#each choisi.noms_conteneurs as c (c)}
                    <option value={c}>{c}</option>
                  {/each}
                </select>
              {/if}
              {#if choisi.redemarrages > 0}
                <label class="inline">
                  <input
                    type="checkbox"
                    bind:checked={logsPrecedents}
                    onchange={() => void lireLeDetail()}
                  />
                  {$trad("k8s.logsPrecedents")}
                </label>
              {/if}
            </div>
          {/if}

          <pre class="contenu">{contenuCharge ? contenu : $trad("k8s.chargement")}</pre>
          <div class="bas">
            <button class="btn small" onclick={() => void lireLeDetail()}>{$trad("k8s.relire")}</button>
            <button class="btn small" onclick={() => void copier()}>{$trad("k8s.copier")}</button>
          </div>
        </aside>
      {/if}
    </div>
  {/if}
</div>

<style>
  .k8s {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 0.6rem;
  }

  .barre {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  .choix { display: flex; gap: 0.4rem; }

  .selecteur {
    display: flex;
    align-items: baseline;
    gap: 0.45rem;
    padding: 0.35rem 0.6rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.85rem;
  }
  .selecteur:hover:not(:disabled) { border-color: var(--border-strong); }
  .selecteur:disabled { opacity: 0.5; cursor: not-allowed; }
  .selecteur .etiquette { color: var(--text-muted); font-size: 0.72rem; text-transform: uppercase; }
  .selecteur .valeur { font-weight: 600; }
  .fleche { color: var(--text-muted); font-size: 0.7rem; }

  .recherche { flex: 1; min-width: 12rem; }

  .comptes { display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; }
  .compte { padding: 0.12rem 0.45rem; border-radius: 999px; }
  .compte.bon { background: var(--success-soft); color: var(--success); }
  .compte.mauvais { background: var(--error-soft); color: var(--error); }
  .compte.fini { color: var(--text-muted); }

  .direct { color: var(--text-muted); font-size: 0.78rem; }
  .direct.actif { color: var(--success); }
  .direct.actif::before { content: "● "; }

  .panneau {
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.5rem;
    max-height: 22rem;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .liste-namespaces { display: flex; flex-direction: column; gap: 0.1rem; margin-top: 0.4rem; }
  .famille {
    color: var(--text-muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0.5rem 0.4rem 0.15rem;
  }

  .entree {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
    width: 100%;
    padding: 0.35rem 0.5rem;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .entree:hover:not(:disabled) { background: var(--bg-tertiary); }
  .entree.active { border-color: var(--accent); background: var(--accent-soft); }
  .entree:disabled { opacity: 0.55; cursor: not-allowed; }
  .detail-entree { color: var(--text-muted); font-size: 0.75rem; }

  .ajouter { align-self: flex-start; margin-top: 0.3rem; }
  .ajout { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.5rem 0.2rem 0.2rem; }
  .etapes { margin: 0; padding-left: 1.1rem; color: var(--text-secondary); font-size: 0.8rem; line-height: 1.6; }
  .colle {
    min-height: 7rem;
    font-family: var(--font-mono);
    font-size: 0.74rem;
    resize: vertical;
  }
  .note { color: var(--text-muted); font-size: 0.74rem; align-self: center; }

  .corps { display: flex; gap: 0.6rem; flex: 1; min-height: 0; }
  .liste { flex: 1; min-width: 0; overflow: auto; display: flex; flex-direction: column; gap: 0.35rem; }
  /* **DANS UNE COLONNE FLEX, LES ENFANTS S'ECRASENT AU LIEU DE DEBORDER.** Avec 78 groupes
     dans une hauteur fixe, chacun se retrouvait haut de douze pixels : des bandes vides, sans
     une lettre lisible. `overflow: auto` ne suffit pas, c'est `flex-shrink` qu'il faut couper.
     Vu au banc, invisible a la relecture. */
  .liste > * { flex: none; }

  .avis {
    color: var(--text-muted);
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
  }

  .groupe {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--bg-secondary);
  }
  .groupe.mauvais { border-color: var(--error); }

  .tete {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    width: 100%;
    padding: 0.45rem 0.6rem;
    background: none;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.88rem;
    text-align: left;
  }
  .tete:hover { background: var(--bg-tertiary); }
  .tete .nom { font-weight: 600; }
  .sorte { color: var(--text-muted); font-size: 0.72rem; text-transform: uppercase; }
  .prets { color: var(--text-secondary); font-variant-numeric: tabular-nums; }
  .version {
    color: var(--accent);
    background: var(--accent-soft);
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    font-size: 0.72rem;
  }
  .nombre { margin-left: auto; color: var(--text-muted); font-size: 0.75rem; }

  .pod {
    display: flex;
    align-items: center;
    border-top: 1px solid var(--border);
  }
  .pod:hover { background: var(--bg-tertiary); }
  .pod.choisi { background: var(--accent-soft); }

  .ligne {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex: 1;
    min-width: 0;
    padding: 0.35rem 0.6rem 0.35rem 1.4rem;
    background: none;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.84rem;
    text-align: left;
  }
  .ligne .nom {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 0.8rem;
  }

  .pastille { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .pastille.bon { background: var(--success); }
  .pastille.mauvais { background: var(--error); }
  .pastille.attente { background: var(--warning); }
  .pastille.fini { background: var(--text-muted); }

  .etat { color: var(--text-secondary); font-size: 0.78rem; }
  .age, .mesure { color: var(--text-muted); font-size: 0.78rem; font-variant-numeric: tabular-nums; }
  .mesure { min-width: 3.6rem; text-align: right; }
  .redemarrages { color: var(--warning); font-size: 0.78rem; }
  .actions { display: flex; gap: 0.2rem; padding-right: 0.5rem; }

  .detail {
    width: min(46%, 40rem);
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-1);
  }
  .tete-detail {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .tete-detail .nom {
    font-family: var(--font-mono);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .onglets { display: flex; gap: 0.2rem; padding: 0.35rem 0.5rem 0; }
  .onglet {
    padding: 0.25rem 0.6rem;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 0.82rem;
  }
  .onglet.actif { color: var(--text-primary); border-bottom-color: var(--accent); }
  .options { display: flex; align-items: center; gap: 0.6rem; padding: 0.4rem 0.5rem 0; font-size: 0.8rem; }
  .inline { display: flex; align-items: center; gap: 0.3rem; color: var(--text-secondary); }

  .contenu {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0.5rem;
    padding: 0.5rem;
    background: var(--bg-primary);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 0.76rem;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .bas { display: flex; gap: 0.3rem; padding: 0 0.5rem 0.5rem; }

  .empty { color: var(--text-muted); text-align: center; padding: 2rem 1rem; }
  .empty .aide { font-size: 0.82rem; margin-top: 0.4rem; }
  .empty .mauvais { color: var(--error); }
</style>
