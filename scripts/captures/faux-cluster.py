#!/usr/bin/env python3
"""Un faux cluster Kubernetes, juste assez vrai pour eprouver l'ecran.

**POURQUOI IL EXISTE.** L'ecran a d'abord ete eprouve sur un vrai cluster : c'est ce qui a
revele le 406 des logs et la liste de namespaces incomplete. Mais un banc qui depend d'une
production est un banc qu'on ne peut pas rejouer — le jour ou ce cluster a refuse toutes les
requetes, kubectl compris, il n'y avait plus aucun moyen de regarder l'interface. Et un depot
public n'a pas a contenir de noms de services d'une entreprise.

Il sert ce dont l'ecran a besoin, et rien de plus :
  GET  /api/v1/namespaces                         la liste, courte
  POST /apis/authorization.k8s.io/v1/selfsubjectrulesreviews  les droits nommes
  GET  /api/v1/namespaces/<ns>/pods               la liste des pods
  GET  ...?watch=1                                le flux des changements
  GET  /apis/metrics.k8s.io/v1beta1/...           le CPU et la memoire
  GET  .../pods/<pod>/log                         les logs, en direct si `follow`
  GET  .../pods/<pod>                             le YAML
  GET  /api/v1/namespaces/<ns>/events             les evenements

Lancement :  python3 faux-cluster.py <port> <dossier ou ecrire le kubeconfig>
"""
import json
import random
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

NAMESPACE = "equipe-demo"
NAMESPACES = [NAMESPACE, "equipe-demo-preprod", "boutique-demo", "outils-demo"]
VERSION = "v2-8f31c07"

# Trois familles, comme sur un vrai namespace : des services declines par marque, des taches
# planifiees, et de quoi voir un incident.
SERVICES = [
    ("web", 4, 4, "Running"),
    ("api-paiement", 2, 2, "Running"),
    ("api-catalogue", 2, 1, "CrashLoopBackOff"),
    ("post-create-alpha-consumer", 1, 1, "Running"),
    ("post-create-beta-consumer", 1, 1, "Running"),
    ("post-create-gamma-consumer", 1, 1, "Running"),
    ("url-change-consumer", 3, 3, "Running"),
    ("url-repairer-consumer", 2, 2, "Running"),
]
TACHES = ["nettoyage-archives", "nettoyage-medias", "nettoyage-journaux", "export-comptable"]

DEBUT = time.time()


def pod(nom, groupe, sorte_proprio, etat, prets, conteneurs, age_s, redemarrages=0):
    statuts = []
    for i in range(conteneurs):
        pret = i < prets
        if etat == "CrashLoopBackOff" and not pret:
            s = {"waiting": {"reason": "CrashLoopBackOff", "message": "back-off 5m0s"}}
        elif etat == "Succeeded":
            s = {"terminated": {"reason": "Completed", "exitCode": 0}}
        else:
            s = {"running": {"startedAt": "2026-09-17T08:00:00Z"}}
        statuts.append({"name": "appli", "ready": pret, "restartCount": redemarrages, "state": s})
    proprio = (
        [{"kind": sorte_proprio, "name": groupe, "uid": "0"}] if sorte_proprio else []
    )
    return {
        "metadata": {
            "name": nom,
            "namespace": NAMESPACE,
            "resourceVersion": str(random.randint(1000, 9999)),
            "ownerReferences": proprio,
        },
        "spec": {
            "nodeName": f"machine-{abs(hash(nom)) % 9 + 1}",
            "containers": [{"name": "appli", "image": f"depot.exemple.test/appli:{VERSION}"}],
        },
        "status": {
            "phase": "Succeeded" if etat == "Succeeded" else "Running",
            "startTime": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - age_s)),
            "containerStatuses": statuts,
        },
    }


def tous_les_pods():
    liste = []
    for nom, total, prets, etat in SERVICES:
        for i in range(total):
            ok = i < prets
            liste.append(
                pod(
                    f"{nom}-5cb5677dcc-{i}xj4{i}",
                    f"{nom}-5cb5677dcc",
                    "ReplicaSet",
                    etat if not ok else "Running",
                    1 if ok else 0,
                    1,
                    3600 * 26,
                    0 if ok else 7,
                )
            )
    # Les taches planifiees laissent derriere elles des dizaines de pods termines.
    for tache in TACHES:
        for i in range(12):
            liste.append(
                pod(
                    f"{tache}-2981{i:04d}-ab{i:02d}z",
                    f"{tache}-2981{i:04d}",
                    "Job",
                    "Succeeded",
                    0,
                    1,
                    3600 * (i + 2),
                )
            )
    # Et quelques orphelins en echec, dont le travail a ete supprime.
    for i in range(4):
        liste.append(pod(f"import-nocturne-29810000-zz{i:02d}q", "", "", "Succeeded", 0, 1, 90000))
    return liste


PODS = tous_les_pods()


class Faux(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):  # silence : le banc a son propre journal
        pass

    def _json(self, charge, code=200):
        corps = json.dumps(charge).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(corps)))
        self.end_headers()
        self.wfile.write(corps)

    def do_POST(self):
        taille = int(self.headers.get("Content-Length", 0))
        self.rfile.read(taille)
        if "selfsubjectrulesreviews" in self.path:
            return self._json(
                {
                    "status": {
                        "resourceRules": [
                            {"verbs": ["get"], "resources": ["namespaces"], "resourceNames": NAMESPACES}
                        ],
                        "incomplete": False,
                    }
                }
            )
        self._json({"kind": "Status", "message": "inconnu"}, 404)

    def do_GET(self):
        chemin = self.path
        if "/log" in chemin:
            return self._logs(chemin)
        if "watch=1" in chemin:
            return self._flux()
        if "/metrics.k8s.io/" in chemin:
            return self._json(
                {
                    "items": [
                        {
                            "metadata": {"name": p["metadata"]["name"]},
                            "containers": [
                                {"usage": {"cpu": f"{abs(hash(p['metadata']['name'])) % 400}m",
                                           "memory": f"{abs(hash(p['metadata']['name'])) % 700 + 60}Mi"}}
                            ],
                        }
                        for p in PODS
                        if p["status"]["phase"] == "Running"
                    ]
                }
            )
        if chemin.startswith("/api/v1/namespaces?") or chemin == "/api/v1/namespaces":
            return self._json(
                {"items": [{"metadata": {"name": n}} for n in NAMESPACES]}
            )
        if "/events" in chemin:
            return self._json(
                {
                    "items": [
                        {
                            "type": "Warning",
                            "reason": "BackOff",
                            "count": 12,
                            "lastTimestamp": "2026-09-17T09:12:00Z",
                            "message": "Back-off restarting failed container appli",
                        },
                        {
                            "type": "Normal",
                            "reason": "Pulled",
                            "count": 1,
                            "lastTimestamp": "2026-09-17T09:00:00Z",
                            "message": "Container image already present on machine",
                        },
                    ]
                }
            )
        if "/pods/" in chemin:
            nom = chemin.split("/pods/")[1].split("?")[0]
            trouve = next((p for p in PODS if p["metadata"]["name"] == nom), None)
            if trouve is None:
                return self._json({"kind": "Status", "message": "pod inconnu"}, 404)
            if "yaml" in self.headers.get("Accept", ""):
                corps = ("apiVersion: v1\nkind: Pod\nmetadata:\n  name: " + nom + "\n").encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/yaml")
                self.send_header("Content-Length", str(len(corps)))
                self.end_headers()
                return self.wfile.write(corps)
            return self._json(trouve)
        if "/pods" in chemin:
            return self._json(
                {"metadata": {"resourceVersion": "4242"}, "items": PODS}
            )
        self._json({"kind": "Status", "message": "route inconnue"}, 404)

    def _flux(self):
        """Envoie un changement toutes les deux secondes, sans fin : c'est un flux."""
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        try:
            for _ in range(60):
                p = json.loads(json.dumps(random.choice(PODS)))
                p["metadata"]["resourceVersion"] = str(random.randint(5000, 9999))
                ligne = (json.dumps({"type": "MODIFIED", "object": p}) + "\n").encode()
                self.wfile.write(f"{len(ligne):X}\r\n".encode() + ligne + b"\r\n")
                self.wfile.flush()
                time.sleep(2)
            self.wfile.write(b"0\r\n\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass

    def _logs(self, chemin):
        suivi = "follow=true" in chemin
        lignes = [
            f"2026-09-17T09:0{i}:12.345678Z appli demarrage de l'etape {i}, tout va bien"
            for i in range(9)
        ] + ["2026-09-17T09:10:00.000000Z appli ERROR connexion refusee, nouvelle tentative"]
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        try:
            for l in lignes:
                octets = (l + "\n").encode()
                self.wfile.write(f"{len(octets):X}\r\n".encode() + octets + b"\r\n")
            self.wfile.flush()
            if suivi:
                for i in range(200):
                    time.sleep(1.5)
                    l = f"2026-09-17T09:20:{i:02d}.000000Z appli ligne vivante numero {i}\n".encode()
                    self.wfile.write(f"{len(l):X}\r\n".encode() + l + b"\r\n")
                    self.wfile.flush()
            self.wfile.write(b"0\r\n\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    port = int(sys.argv[1])
    dossier = sys.argv[2]
    kubeconfig = f"""apiVersion: v1
kind: Config
current-context: cluster-demo
clusters:
  - name: cluster-demo
    cluster:
      server: "http://127.0.0.1:{port}"
contexts:
  - name: cluster-demo
    context:
      cluster: cluster-demo
      user: cluster-demo
      namespace: {NAMESPACE}
users:
  - name: cluster-demo
    user:
      token: jeton-de-demonstration
"""
    chemin = f"{dossier}/kubeconfig-demo.yaml"
    with open(chemin, "w", encoding="utf-8") as f:
        f.write(kubeconfig)
    print(chemin, flush=True)
    serveur = ThreadingHTTPServer(("127.0.0.1", port), Faux)
    threading.Thread(target=serveur.serve_forever, daemon=True).start()
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    main()
