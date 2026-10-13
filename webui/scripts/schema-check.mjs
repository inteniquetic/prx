// T205: the generated JSON Schema must accept the Prx.toml in the repo, and
// must reject a config with a wrong type. Run after `make schema`.
import { readFileSync } from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import { parse } from 'smol-toml';

const root = new URL('../../', import.meta.url);
const read = (path) => readFileSync(new URL(path, root), 'utf8');
const schema = JSON.parse(read('docs/prx.schema.json'));
const ajv = new Ajv2020({ strict: false, allErrors: true, validateFormats: false });
const validate = ajv.compile(schema);

const problems = [];
const config = parse(read('Prx.toml'));
if (!validate(config)) {
  problems.push(`Prx.toml does not satisfy the schema: ${JSON.stringify(validate.errors?.slice(0, 5))}`);
}
const broken = structuredClone(config);
broken.server = { ...(broken.server ?? {}), listen: 8080 };
if (validate(broken)) {
  problems.push('a numeric server.listen was accepted: the schema checks nothing');
}
if (problems.length > 0) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log('schema-check passed: Prx.toml validates, a wrong type is rejected');
