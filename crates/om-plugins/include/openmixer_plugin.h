/* OpenMixer plugin C ABI, versions 1 and 2. The Rust definition is crates/om-plugins/src/abi.rs;
 * the rules (threads, ownership, no unwinding) are in that file's module docs and in
 * docs/spikes/DECISIONS.md. A plugin library exports `openmixer_plugin_entry` and nothing else is
 * needed. Return a version 1 table for a host that predates version 2, or a version 2 table. */
#ifndef OPENMIXER_PLUGIN_H
#define OPENMIXER_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENMIXER_ABI_VERSION 2
#define OPENMIXER_ABI_MIN 1

/* Status codes. process and prepare: non-zero stops the host calling that instance (it passes
 * audio through). param_set: REJECTED is not a fault. */
#define OPENMIXER_STATUS_OK 0
#define OPENMIXER_STATUS_REJECTED 1
#define OPENMIXER_STATUS_FAULT 2

/* Version 1 table: no prepare, no parameters, process returns nothing. */
typedef struct OpenMixerPluginVTableV1 {
  uint32_t abi_version; /* 1 */
  void (*info)(char *buf, size_t len);
  void *(*create)(void);
  void (*destroy)(void *instance);
  void (*process)(void *instance, float *samples, size_t frames);
} OpenMixerPluginVTableV1;

/* Version 2 table. Leave prepare and the four param_* pointers NULL when unsupported; the four
 * parameter functions go together. */
typedef struct OpenMixerPluginVTable {
  uint32_t abi_version; /* 2 */
  /* Writes a NUL-terminated JSON object: {"id","name","version"} required; "stage_id", "kind"
   * ("fx"|"pre"|"post") and "param_prefix" optional. At most len bytes with the NUL. */
  void (*info)(char *buf, size_t len);
  void *(*create)(void);
  void (*destroy)(void *instance);
  /* In place, mono, frames <= the max_block given to prepare. Real-time safe. */
  int32_t (*process)(void *instance, float *samples, size_t frames);
  int32_t (*prepare)(void *instance, double sample_rate, size_t max_block);
  uint32_t (*param_count)(void);
  /* Writes {"id","name","min","max","default"} for parameter `index`. */
  void (*param_info)(uint32_t index, char *buf, size_t len);
  double (*param_get)(void *instance, uint32_t index);
  /* Called from the audio thread between buffers; real-time safe. */
  int32_t (*param_set)(void *instance, uint32_t index, double value);
} OpenMixerPluginVTable;

/* The one exported symbol. The table must stay valid while the library is loaded. */
const OpenMixerPluginVTable *openmixer_plugin_entry(void);

#ifdef __cplusplus
}
#endif

#endif
