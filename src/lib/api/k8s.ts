import { invoke } from "../coquille";
import type { Pod } from "../k8s/vue";

/** Un contexte du kubeconfig, tel qu'il s'affiche dans le selecteur de cluster. */
export interface Contexte {
  nom: string;
  cluster: string;
  serveur: string;
  namespace: string | null;
  courant: boolean;
  /** Rempli quand Cockpit ne sait pas parler a ce cluster : la raison, affichee telle quelle. */
  obstacle: string | null;
}

export interface Vue {
  pods: Pod[];
  sans_mesures: string | null;
  version: string;
}

export const k8sContextes = () => invoke<Contexte[]>("k8s_contextes");
export const k8sNamespaces = (contexte: string) => invoke<string[]>("k8s_namespaces", { contexte });
export const k8sPods = (contexte: string, namespace: string) =>
  invoke<Vue>("k8s_pods", { contexte, namespace });
export const k8sLogs = (
  contexte: string,
  namespace: string,
  pod: string,
  conteneur: string | null,
  lignes: number,
  precedent: boolean,
) => invoke<string>("k8s_logs", { contexte, namespace, pod, conteneur, lignes, precedent });
export const k8sEvenements = (contexte: string, namespace: string, pod: string) =>
  invoke<Record<string, unknown>[]>("k8s_evenements", { contexte, namespace, pod });
export const k8sYaml = (contexte: string, namespace: string, pod: string) =>
  invoke<string>("k8s_yaml", { contexte, namespace, pod });
/** Ce que la fusion d'un kubeconfig a produit. */
export interface Ajout {
  ajoutes: string[];
  deja_la: string[];
  renouveles: string[];
  renommes: string[];
}

/**
 * Ajoute un cluster a partir du kubeconfig telecharge depuis son interface web.
 *
 * Le contenu part au backend et n'en revient jamais : il porte un jeton d'acces.
 */
export const k8sAjouterUnCluster = (kubeconfig: string) =>
  invoke<Ajout>("k8s_ajouter_un_cluster", { kubeconfig });

/** `kubectl` est-il installe ? Seul le bouton « ouvrir un shell » en depend. */
export const k8sKubectlPresent = () => invoke<boolean>("k8s_kubectl_present");
