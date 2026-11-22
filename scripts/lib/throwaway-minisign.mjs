/**
 * A throwaway minisign key pair and signer, in the format Tauri's updater
 * signer writes (base64 of the key and signature files, an Ed25519 signature
 * over the BLAKE2b-512 of the file): what a local channel the apps' updaters
 * read is signed with in tests and the e2e, never a release.
 */
import { createHash, generateKeyPairSync, sign } from "node:crypto";

export function throwawayMinisign(keyId = Buffer.from("0123456789abcdef", "hex")) {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(12);
  const keyFile = `untrusted comment: minisign public key: throwaway\n${Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64")}\n`;
  const signFile = (file, comment = "timestamp:1\tfile:archive") => {
    const signature = sign(null, createHash("blake2b512").update(file).digest(), privateKey);
    const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const text = [
      "untrusted comment: signature from a throwaway key",
      Buffer.concat([Buffer.from("ED"), keyId, signature]).toString("base64"),
      `trusted comment: ${comment}`,
      global.toString("base64"),
      "",
    ].join("\n");
    return Buffer.from(text).toString("base64");
  };
  return { publicKey: Buffer.from(keyFile).toString("base64"), signFile };
}
