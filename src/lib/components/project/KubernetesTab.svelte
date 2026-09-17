<script lang="ts">
  /**
   * L'ecran des pods d'un namespace.
   *
   * **CE QUI FAIT LA DIFFERENCE AVEC L'ECRAN D'EN FACE : L'ORDRE, LES FAMILLES ET LE FLUX.** Un
   * namespace reel contient 298 pods pour 66 services, la plupart declines par marque. Tout est
   * affiche, mais range : des filtres en haut, les services d'une meme famille sous une seule
   * ligne, ce qui reclame une action d'abord. Et la liste se met a jour par un FLUX : on ne
   * recharge jamais les 6,3 Mo que pese sa lecture complete.
   */
  import { onMount, onDestroy } from "svelte";
  import { ecouter, invoke, type Detacher } from "../../coquille";
  import { activeTab, pendingTerminalCommand } from "../../stores/ui";
  import { notify } from "../../stores/toast";
  import { signalerErreur } from "../../stores/errors";
  import { getAppSettings, setAppSetting } from "../../api/recorder";
  import { trad } from "../../i18n";
  import PodJournal from "./PodJournal.svelte";
  import {
    k8sContextes, k8sNamespaces, k8sPods, k8sEvenements, k8sYaml,
    k8sKubectlPresent, k8sAjouterUnCluster, type Contexte,
  } from "../../api/k8s";
  import {
    filtrer, grouper, formaterCpu, formaterRam, age, grouperLesNamespaces,
    appliquer, appliquerLesMesures, cibleDeDemarrage, enFamilles, comptesDesFiltres,
    appliquerLeFiltre, peutAllerA, type Pod, type Mesure, type Filtre,
  } from "../../k8s/vue";

  let { name }: { name: string } = $props();

  let contextes: Contexte[] = $state([]);
  let contexte = $state("");
  let namespaces: string[] = $state([]);
  let namespace = $state("");
  let pods: Pod[] = $state([]);
  let sansMesures: string | null = $state(null);
  let recherche = $state("");
  let filtre: Filtre = $state("tout");
  let chargement = $state(false);
  let panne: string | null = $state(null);
  let enDirect = $state(false);
  let ouverts = $state(new Set<string>());
  let kubectl = $state(false);

  let ouvert: "cluster" | "namespace" | null = $state(null);
  let chercheNamespace = $state("");
  let ajoutOuvert = $state(false);
  let texteColle = $state("");
  let ajoutEnCours = $state(false);

  /// Le pod dont on regarde le detail, et ce qu'on y regarde.
  let choisi: Pod | null = $state(null);
  let volet: "logs" | "evenements" | "yaml" = $state("logs");
  let contenu = $state("");
  let contenuCharge = $state(false);

  let maintenant = $state(Date.now());
  let horloge: ReturnType<typeof setInterval> | null = null;
  let detacheurs: Detacher[] = [];

  const cle = $derived(`k8s.cible.${name}`);
  const filtres = $derived(appliquerLeFiltre(pods, filtre));
  const visibles = $derived(filtrer(filtres, recherche));
  const familles = $derived(enFamilles(grouper(visibles)));
  const comptes = $derived(comptesDesFiltres(pods));
  const contexteActif = $derived(contextes.find((c) => c.nom === contexte));
  const famillesDeNamespaces = $derived(
    grouperLesNamespaces(
      namespaces.filter((n) => n.toLowerCase().includes(chercheNamespace.toLowerCase())),
    ),
  );
  /// La reference des jauges : le plus gros consommateur affiche. Sans reference commune, deux
  /// barres de meme longueur diraient deux choses differentes.
  const cpuMax = $derived(Math.max(1, ...familles.flatMap((f) => f.groupes.map((g) => g.cpu ?? 0))));
  const ramMax = $derived(Math.max(1, ...familles.flatMap((f) => f.groupes.map((g) => g.ram ?? 0))));
  /// Une recherche ou un filtre ouvre ce qu'il trouve : un resultat replie ne se montre pas.
  const toutOuvert = $derived(recherche.trim().length > 0 || filtre !== "tout");

  const LIBELLES: { id: Filtre; libelle: Parameters<typeof $trad>[0] }[] = [
    { id: "tout", libelle: "k8s.filtreTout" },
    { id: "deployments", libelle: "k8s.filtreServices" },
    { id: "cronjobs", libelle: "k8s.filtreTaches" },
    { id: "avoir", libelle: "k8s.filtreAvoir" },
    { id: "marche", libelle: "k8s.filtreMarche" },
    { id: "termines", libelle: "k8s.filtreTermines" },
  ];

  /// Au-dela, on compte au lieu de dessiner : cent pastilles ne se lisent plus.
  const PASTILLES_MAX = 10;

  onMount(() => {
    horloge = setInterval(() => (maintenant = Date.now()), 1000);
    void demarrer();
    return () => {
      if (horloge) clearInterval(horloge);
    };
  });

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
    const depart =
      servables.find((c) => c.nom === vise.contexte) ??
      servables.find((c) => c.courant) ??
      servables[0];
    if (!depart) return;
    await choisirLeCluster(depart.nom, vise.namespace ?? null, false);
  }

  /**
   * `enregistrer` distingue le GESTE de la restauration.
   *
   * **UNE RESTAURATION N'ECRIT JAMAIS.** Sinon, le jour ou le namespace retenu ne se retrouve
   * pas dans la liste, l'ecran remplace le choix de l'utilisateur par son defaut et l'efface
   * pour de bon. C'est ce qui a ete signale : un projet revenait sur le namespace du contexte.
   */
  async function choisirLeCluster(
    nom: string,
    namespaceVise: string | null = null,
    enregistrer = true,
  ) {
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
    await choisirLeNamespace(cibleDeDemarrage(namespaces, namespaceVise, duContexte), enregistrer);
  }

  async function choisirLeNamespace(nom: string, enregistrer = true) {
    namespace = nom;
    ouvert = null;
    chercheNamespace = "";
    choisi = null;
    ouverts = new Set();
    detacher();
    if (enregistrer) {
      void setAppSetting(cle, JSON.stringify({ contexte, namespace })).catch((e) =>
        signalerErreur("k8s.reglage", String(e)),
      );
    }
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

  function basculer(quoi: string) {
    const suite = new Set(ouverts);
    if (suite.has(quoi)) suite.delete(quoi);
    else suite.add(quoi);
    ouverts = suite;
  }

  async function ouvrirLeDetail(pod: Pod, lequel: "logs" | "evenements" | "yaml" = "logs") {
    choisi = pod;
    volet = lequel;
    if (lequel !== "logs") await lireLeDetail();
  }

  async function lireLeDetail() {
    if (!choisi || volet === "logs") return;
    contenuCharge = false;
    contenu = "";
    try {
      if (volet === "yaml") {
        contenu = await k8sYaml(contexte, namespace, choisi.nom);
      } else {
        const liste = await k8sEvenements(contexte, namespace, choisi.nom);
        contenu = liste.length === 0
          ? $trad("k8s.aucunEvenement")
          : liste.map(formaterEvenement).join("\n\n");
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
    return `${heure}  ${e.type ?? ""}  ${e.reason ?? ""}${fois}\n${e.message ?? ""}`;
  }

  /// Ouvre un shell DANS le conteneur, par un vrai terminal Cockpit : on y retrouve ses
  /// volets, son historique et son copier-coller. Seul ce bouton demande `kubectl`.
  function ouvrirUnShell(pod: Pod) {
    const c = pod.noms_conteneurs[0];
    const ou = c ? ` -c ${c}` : "";
    const commande =
      `kubectl --context ${contexte} -n ${namespace} exec -it ${pod.nom}${ou}` +
      ` -- sh -c '[ -x /bin/bash ] && exec bash || exec sh'`;
    pendingTerminalCommand.set({ project: name, command: commande });
    activeTab.set("terminal");
  }

  async function ajouterUnCluster() {
    const texte = texteColle.trim();
    if (!texte) return;
    ajoutEnCours = true;
    try {
      const quoi = await k8sAjouterUnCluster(texte);
      // **ON EFFACE CE QU'ON VIENT DE COLLER** : un jeton d'acces n'a pas a rester affiche.
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

  /// Un fichier depose vaut un collage : c'est celui que l'interface web fait telecharger.
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

  function couleurDe(pod: Pod): string {
    if (pod.ennuyeux) return "mauvais";
    if (pod.etat === "Succeeded") return "fini";
    if (pod.etat === "Running" && pod.prets === pod.conteneurs) return "bon";
    return "attente";
  }
</script>

<div class="k8s">
  <div class="barre">
    <div class="cibles">
      <button
        class="pilule"
        class:actif={ouvert === "cluster"}
        onclick={() => (ouvert = ouvert === "cluster" ? null : "cluster")}
        disabled={contextes.length === 0}
        title={contexteActif?.serveur ?? ""}
      >
        <span class="pilule-cle">{$trad("k8s.cluster")}</span>
        <span class="pilule-valeur">{contexte || $trad("k8s.aucun")}</span>
        <span class="chevron">▾</span>
      </button>
      <button
        class="pilule"
        class:actif={ouvert === "namespace"}
        onclick={() => (ouvert = ouvert === "namespace" ? null : "namespace")}
        disabled={namespaces.length === 0 && !namespace}
      >
        <span class="pilule-cle">{$trad("k8s.namespace")}</span>
        <span class="pilule-valeur">{namespace || "—"}</span>
        <span class="chevron">▾</span>
      </button>
    </div>

    <input
      class="input recherche"
      type="search"
      bind:value={recherche}
      placeholder={$trad("k8s.chercher")}
      disabled={pods.length === 0}
    />

    <div class="vivant">
      <span class="direct" class:actif={enDirect} title={enDirect ? $trad("k8s.directAide") : ""}>
        {enDirect ? $trad("k8s.direct") : $trad("k8s.arrete")}
      </span>
      <button class="btn small ghost" onclick={() => void charger()} disabled={chargement}>
        {$trad("k8s.relire")}
      </button>
    </div>
  </div>

  {#if pods.length > 0}
    <div class="filtres">
      {#each LIBELLES as f (f.id)}
        {@const n = comptes[f.id]}
        <button
          class="filtre {f.id}"
          class:actif={filtre === f.id}
          disabled={n === 0 && f.id !== "tout"}
          onclick={() => (filtre = f.id)}
        >
          {$trad(f.libelle)}<span class="compte">{n}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if ouvert === "cluster"}
    <div class="panneau">
      {#each contextes as c (c.nom)}
        <button
          class="entree"
          class:choisie={c.nom === contexte}
          disabled={!!c.obstacle}
          onclick={() => void choisirLeCluster(c.nom)}
          title={c.obstacle ?? c.serveur}
        >
          <span class="entree-nom">{c.nom}</span>
          <span class="entree-detail">{c.obstacle ?? c.serveur}</span>
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
          <div class="ajout-bas">
            <button
              class="btn small primary"
              onclick={() => void ajouterUnCluster()}
              disabled={ajoutEnCours || !texteColle.trim()}
            >{$trad("k8s.ajouterBouton")}</button>
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
      {#if peutAllerA(chercheNamespace, namespaces)}
        <!-- **UN NAMESPACE ABSENT DE LA LISTE RESTE ATTEIGNABLE.** La liste vient des droits, et
             un cluster peut refuser de l'etablir tout en donnant acces aux pods. -->
        <button class="btn small primary aller" onclick={() => void choisirLeNamespace(chercheNamespace.trim())}>
          {$trad("k8s.allerA", { nom: chercheNamespace.trim() })}
        </button>
      {/if}
      {#if namespaces.length <= 1}
        <p class="note">{$trad("k8s.listeIncomplete")}</p>
      {/if}
      <div class="namespaces">
        {#each famillesDeNamespaces as famille (famille.famille)}
          {#if famille.famille}<div class="famille-titre">{famille.famille}</div>{/if}
          {#each famille.noms as n (n)}
            <button
              class="entree"
              class:choisie={n === namespace}
              onclick={() => void choisirLeNamespace(n)}
            >
              <span class="entree-nom">{n}</span>
            </button>
          {/each}
        {/each}
      </div>
    </div>
  {/if}

  <!-- **UNE PANNE PASSE AVANT « IL N'Y A RIEN ».** Dans l'autre ordre, un backend qui refuse de
       repondre affichait « aucun cluster » : l'ecran envoyait chercher un kubeconfig correct au
       lieu de dire ce qui s'etait passe. -->
  {#if panne}
    <div class="vide">
      <p class="erreur">{panne}</p>
      <button class="btn" onclick={() => void charger()}>{$trad("k8s.reessayer")}</button>
    </div>
  {:else if contextes.length === 0}
    <div class="vide">
      <p>{$trad("k8s.aucunKubeconfig")}</p>
      <p class="aide">{$trad("k8s.aucunKubeconfigAide")}</p>
    </div>
  {:else}
    <div class="corps">
      <div class="liste">
        {#if sansMesures}
          <p class="avis" title={sansMesures}>{$trad("k8s.sansMesures")}</p>
        {/if}
        {#if chargement && pods.length === 0}
          <div class="vide">{$trad("k8s.chargement")}</div>
        {:else if familles.length === 0}
          <div class="vide">{recherche ? $trad("k8s.aucunResultat") : $trad("k8s.aucunPod")}</div>
        {/if}

        {#each familles as famille (famille.sorte + famille.nom + famille.groupes[0].nom)}
          {@const cleFamille = `f:${famille.sorte}:${famille.nom}`}
          {@const familleOuverte = toutOuvert || ouverts.has(cleFamille) || !famille.nom}

          {#if famille.nom}
            <button
              class="famille"
              class:alerte={famille.ennuyeux}
              onclick={() => basculer(cleFamille)}
            >
              <span class="chevron">{familleOuverte ? "▾" : "▸"}</span>
              <span class="famille-nom">{famille.nom}</span>
              <span class="etiquette">{$trad("k8s.servicesN", { n: famille.groupes.length })}</span>
              <span class="sante">{famille.prets}/{famille.attendus}</span>
              <span class="espace"></span>
              <span class="mesure">{formaterCpu(famille.cpu)}</span>
              <span class="mesure">{formaterRam(famille.ram)}</span>
            </button>
          {/if}

          {#if familleOuverte}
            {#each famille.groupes as g (g.sorte + g.nom)}
              {@const cleGroupe = `g:${g.sorte}:${g.nom}`}
              {@const deplie = toutOuvert || ouverts.has(cleGroupe)}
              <article class="carte" class:alerte={g.ennuyeux} class:dans-famille={!!famille.nom}>
                <button class="tete" onclick={() => basculer(cleGroupe)}>
                  <span class="chevron">{deplie ? "▾" : "▸"}</span>

                  <span class="pastilles">
                    {#each g.pods.slice(0, PASTILLES_MAX) as p (p.nom)}
                      <span class="pastille {couleurDe(p)}" title={p.etat}></span>
                    {/each}
                    {#if g.pods.length > PASTILLES_MAX}
                      <span class="reste">+{g.pods.length - PASTILLES_MAX}</span>
                    {/if}
                  </span>

                  <span class="nom">{g.nom}</span>
                  {#if g.sorte}<span class="sorte">{g.sorte}</span>{/if}
                  <span class="sante" class:incomplet={g.prets < g.attendus}>
                    {g.prets}/{g.attendus}
                  </span>
                  {#each g.versions.slice(0, 2) as v (v)}
                    <span class="version">{v}</span>
                  {/each}
                  {#if g.versions.length > 2}
                    <span class="version">+{g.versions.length - 2}</span>
                  {/if}

                  <span class="espace"></span>

                  <span class="jauge" title={$trad("k8s.cpu")}>
                    <span class="remplissage" style="width:{Math.min(100, ((g.cpu ?? 0) / cpuMax) * 100)}%"></span>
                    <span class="valeur">{formaterCpu(g.cpu)}</span>
                  </span>
                  <span class="jauge" title={$trad("k8s.ram")}>
                    <span class="remplissage ram" style="width:{Math.min(100, ((g.ram ?? 0) / ramMax) * 100)}%"></span>
                    <span class="valeur">{formaterRam(g.ram)}</span>
                  </span>
                </button>

                {#if deplie}
                  <div class="pods">
                    {#each g.pods as p (p.nom)}
                      <div class="pod" class:choisi={choisi?.nom === p.nom}>
                        <button class="ligne" onclick={() => void ouvrirLeDetail(p)}>
                          <span class="pastille {couleurDe(p)}"></span>
                          <span class="pod-nom">{p.nom}</span>
                          <span class="pod-etat" class:mauvais={p.ennuyeux}>{p.etat}</span>
                          <span class="pod-age">{age(p.depuis, maintenant)}</span>
                          {#if p.redemarrages > 0}
                            <span class="redemarrages" title={$trad("k8s.redemarrages")}>
                              ⟳ {p.redemarrages}
                            </span>
                          {/if}
                          <span class="espace"></span>
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
                  </div>
                {/if}
              </article>
            {/each}
          {/if}
        {/each}
      </div>

      {#if choisi}
        <aside class="detail">
          <div class="detail-tete">
            <div class="detail-titre">
              <span class="pastille {couleurDe(choisi)}"></span>
              <span class="detail-nom">{choisi.nom}</span>
            </div>
            <button class="btn small ghost" onclick={() => (choisi = null)} aria-label={$trad("k8s.fermer")}>✕</button>
          </div>

          <div class="detail-faits">
            <span>{choisi.etat}</span>
            <span>{age(choisi.depuis, maintenant)}</span>
            {#if choisi.version}<span class="version">{choisi.version}</span>{/if}
            {#if choisi.machine}<span title={$trad("k8s.machine")}>⌗ {choisi.machine}</span>{/if}
            {#if choisi.redemarrages > 0}<span class="redemarrages">⟳ {choisi.redemarrages}</span>{/if}
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
            <!-- Changer de pod remonte un journal NEUF : sans cette cle, les lignes du pod
                 precedent resteraient a l'ecran sous un autre nom. -->
            {#key choisi.nom}
              <PodJournal
                {contexte}
                {namespace}
                pod={choisi.nom}
                conteneurs={choisi.noms_conteneurs}
                redemarrages={choisi.redemarrages}
              />
            {/key}
          {:else}
            <pre class="contenu">{contenuCharge ? contenu : $trad("k8s.chargement")}</pre>
          {/if}
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
    gap: 0.55rem;
  }

  /* ── La barre du haut ─────────────────────────────────────────────────────── */
  .barre { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; }
  .cibles { display: flex; gap: 0.4rem; }

  .pilule {
    display: flex;
    align-items: baseline;
    gap: 0.45rem;
    padding: 0.38rem 0.7rem;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.85rem;
    transition: border-color 0.12s ease, background 0.12s ease;
  }
  .pilule:hover:not(:disabled) { border-color: var(--border-strong); }
  .pilule.actif { border-color: var(--accent); background: var(--accent-soft); }
  .pilule:disabled { opacity: 0.5; cursor: not-allowed; }
  .pilule-cle { color: var(--text-muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .pilule-valeur { font-weight: 600; }
  .chevron { color: var(--text-muted); font-size: 0.68rem; flex: none; }

  .recherche { flex: 1; min-width: 11rem; border-radius: 999px; }
  .vivant { display: flex; align-items: center; gap: 0.5rem; }
  .direct { color: var(--text-muted); font-size: 0.76rem; }
  .direct.actif { color: var(--success); }
  .direct.actif::before { content: "● "; }

  /* ── Les filtres ──────────────────────────────────────────────────────────── */
  .filtres { display: flex; gap: 0.35rem; flex-wrap: wrap; }
  .filtre {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.26rem 0.7rem;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 0.78rem;
    transition: color 0.12s ease, border-color 0.12s ease, background 0.12s ease;
  }
  .filtre:hover:not(:disabled) { color: var(--text-primary); border-color: var(--border-strong); }
  .filtre:disabled { opacity: 0.4; cursor: default; }
  .filtre .compte {
    padding: 0.02rem 0.4rem;
    border-radius: 999px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }
  .filtre.actif { color: var(--text-primary); border-color: var(--accent); background: var(--accent-soft); }
  .filtre.actif .compte { background: var(--accent); color: #fff; }
  .filtre.avoir.actif { border-color: var(--error); background: var(--error-soft); }
  .filtre.avoir.actif .compte { background: var(--error); }

  /* ── Les panneaux de choix ────────────────────────────────────────────────── */
  .panneau {
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.5rem;
    max-height: 24rem;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.25);
  }
  .panneau > * { flex: none; }
  .namespaces { display: flex; flex-direction: column; gap: 0.08rem; margin-top: 0.4rem; }
  .famille-titre {
    color: var(--text-muted);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0.55rem 0.45rem 0.15rem;
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
  .entree.choisie { border-color: var(--accent); background: var(--accent-soft); }
  .entree:disabled { opacity: 0.55; cursor: not-allowed; }
  .entree-detail { color: var(--text-muted); font-size: 0.74rem; }

  .ajouter { align-self: flex-start; margin-top: 0.3rem; }
  .aller { align-self: flex-start; margin-top: 0.35rem; }
  .ajout { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.5rem 0.2rem 0.2rem; }
  .etapes { margin: 0; padding-left: 1.1rem; color: var(--text-secondary); font-size: 0.8rem; line-height: 1.6; }
  .colle { min-height: 7rem; font-family: var(--font-mono); font-size: 0.74rem; resize: vertical; }
  .ajout-bas { display: flex; gap: 0.35rem; align-items: center; flex-wrap: wrap; }
  .note { color: var(--text-muted); font-size: 0.74rem; }

  /* ── La liste ─────────────────────────────────────────────────────────────── */
  .corps { display: flex; gap: 0.7rem; flex: 1; min-height: 0; }
  .liste {
    flex: 1;
    min-width: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding-right: 0.15rem;
  }
  /* **DANS UNE COLONNE FLEX, LES ENFANTS S'ECRASENT AU LIEU DE DEBORDER.** Avec 78 groupes
     dans une hauteur fixe, chacun se retrouvait haut de douze pixels : des bandes vides, sans
     une lettre lisible. `overflow: auto` ne suffit pas, c'est `flex-shrink` qu'il faut couper. */
  .liste > * { flex: none; }

  .avis {
    color: var(--text-muted);
    font-size: 0.78rem;
    padding: 0.35rem 0.6rem;
    margin: 0;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
  }

  .famille {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
    padding: 0.4rem 0.6rem;
    margin-top: 0.25rem;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 0.8rem;
    text-align: left;
  }
  .famille:hover { color: var(--text-primary); }
  .famille-nom { font-weight: 600; }
  .famille.alerte .famille-nom { color: var(--error); }
  .etiquette {
    padding: 0.03rem 0.45rem;
    border-radius: 999px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-size: 0.7rem;
  }

  .carte {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
    overflow: hidden;
    transition: border-color 0.12s ease;
  }
  .carte:hover { border-color: var(--border-strong); }
  .carte.alerte { border-color: var(--error); }
  .carte.dans-famille { margin-left: 1.1rem; }

  .tete {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    width: 100%;
    padding: 0.5rem 0.7rem;
    background: none;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.87rem;
    text-align: left;
  }
  .tete:hover { background: var(--bg-tertiary); }
  .nom { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sorte {
    color: var(--text-muted);
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex: none;
  }
  .sante { color: var(--text-secondary); font-size: 0.78rem; font-variant-numeric: tabular-nums; flex: none; }
  .sante.incomplet { color: var(--warning); }
  .version {
    padding: 0.04rem 0.45rem;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 0.7rem;
    flex: none;
  }
  .espace { flex: 1; }

  .pastilles { display: inline-flex; align-items: center; gap: 0.16rem; flex: none; }
  .pastille { width: 7px; height: 7px; border-radius: 2px; flex: none; }
  .pastille.bon { background: var(--success); }
  .pastille.mauvais { background: var(--error); }
  .pastille.attente { background: var(--warning); }
  .pastille.fini { background: var(--text-muted); opacity: 0.55; }
  .reste { color: var(--text-muted); font-size: 0.68rem; margin-left: 0.1rem; }

  .jauge {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    width: 4.6rem;
    height: 1.15rem;
    padding: 0 0.35rem;
    border-radius: var(--radius-sm);
    background: var(--bg-tertiary);
    flex: none;
    overflow: hidden;
  }
  /* **DEUX CHOSES NE PARTAGENT PAS UN NOM DE CLASSE.** Ce remplissage s'est d'abord appele
     `.barre`, comme la barre du haut de l'ecran : celle-ci heritait donc de `position:
     absolute` et partait se coller dans le coin de la fenetre, par-dessus la barre laterale.
     Invisible a la relecture, evident des qu'on encadre les boites. */
  .remplissage { position: absolute; inset: 0 auto 0 0; background: var(--accent-soft); }
  .remplissage.ram { background: var(--success-soft); }
  .valeur {
    position: relative;
    color: var(--text-secondary);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }

  .pods { border-top: 1px solid var(--border); }
  .pod { display: flex; align-items: center; }
  .pod + .pod { border-top: 1px solid var(--border); }
  .pod:hover { background: var(--bg-tertiary); }
  .pod.choisi { background: var(--accent-soft); }

  .ligne {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex: 1;
    min-width: 0;
    padding: 0.32rem 0.7rem 0.32rem 1.6rem;
    background: none;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .pod-nom {
    font-family: var(--font-mono);
    font-size: 0.76rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pod-etat { color: var(--text-secondary); font-size: 0.76rem; flex: none; }
  .pod-etat.mauvais { color: var(--error); }
  .pod-age, .mesure { color: var(--text-muted); font-size: 0.76rem; font-variant-numeric: tabular-nums; flex: none; }
  .mesure { min-width: 3.4rem; text-align: right; }
  .redemarrages { color: var(--warning); font-size: 0.76rem; flex: none; }
  .actions { display: flex; gap: 0.2rem; padding-right: 0.55rem; }

  /* ── Le detail ────────────────────────────────────────────────────────────── */
  .detail {
    width: min(50%, 44rem);
    display: flex;
    flex-direction: column;
    min-height: 0;
    gap: 0.5rem;
    padding: 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-1);
  }
  .detail-tete { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .detail-titre { display: flex; align-items: center; gap: 0.5rem; min-width: 0; }
  .detail-nom {
    font-family: var(--font-mono);
    font-size: 0.82rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail-faits {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    color: var(--text-muted);
    font-size: 0.76rem;
  }
  .onglets { display: flex; gap: 0.15rem; border-bottom: 1px solid var(--border); }
  .onglet {
    padding: 0.3rem 0.7rem;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 0.82rem;
  }
  .onglet:hover { color: var(--text-primary); }
  .onglet.actif { color: var(--text-primary); border-bottom-color: var(--accent); }

  .contenu {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: 0.6rem;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .vide { color: var(--text-muted); text-align: center; padding: 2.5rem 1rem; }
  .vide .aide { font-size: 0.82rem; margin-top: 0.45rem; }
  .vide .erreur { color: var(--error); }
</style>
