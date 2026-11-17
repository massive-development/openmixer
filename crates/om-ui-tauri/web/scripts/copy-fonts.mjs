// Copies the bundled fonts (and their licence texts) into public/fonts, so the build serves the
// same files as the native window instead of a second committed copy.
import { cpSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../../../../resources/fonts/', import.meta.url));
const target = fileURLToPath(new URL('../public/fonts/', import.meta.url));

cpSync(source, target, { recursive: true });
