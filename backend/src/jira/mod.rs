//! Jira Server / Data Center : lister ses tickets et les faire avancer depuis Cockpit.
//!
//! **LE JETON NE QUITTE JAMAIS LE BACKEND.** L'interface recoit `jeton_pose`, pas la valeur :
//! meme regle que les cles d'API des fournisseurs d'IA.

pub mod branche;
