import {readFileSync,writeFileSync} from 'node:fs';
const input=JSON.parse(readFileSync('docs/model-schemas.json'));
const schemas={};
for(const [name,schema] of Object.entries(input)){
  const value=JSON.parse(JSON.stringify(schema).replaceAll('"#/$defs/','"#/components/schemas/'+name+'/$defs/'));
  delete value.$schema;schemas[name]=value;
}
const obj=(properties,required=Object.keys(properties))=>({type:'object',properties,required,additionalProperties:false});
const string={type:'string'};const integer={type:'integer',minimum:0};
const ref=name=>({'$ref':`#/components/schemas/${name}`});
const commands={
  scenarios:ref('Scenario'),arm:{oneOf:[ref('Scenario'),obj({scenario_id:string})]},devices:ref('Device'),
  purchase:obj({device_id:string,amount:{...integer,minimum:1,maximum:999999999},merchant:string},['device_id','amount']),
  standalone:obj({scenario:ref('Scenario'),amount:{...integer,minimum:1,maximum:999999999}}),
  action:obj({operation_id:string,event:{type:'string',enum:['card_presented','customer_confirmed','bank_approved','bank_declined','terminal_confirmed','customer_cancelled','device_disconnected','reversed']}}),
  advance:obj({ms:{...integer,maximum:3600000}}),transport:obj({online:{type:'boolean'}}),reset:obj({}),
  assert:obj({requests:integer,accepted:integer,approvals:integer,reversals:integer,delivered:integer,queue_empty:{type:'boolean'},idle:{type:'boolean'}},[]),
};
const reads=['health','profile','errors','state','journal','devices','operations','scenarios','queue','events','transport'];
const paths={};
for(const resource of new Set([...reads,...Object.keys(commands)])){
  const path={};
  if(reads.includes(resource))path.get={operationId:`get_${resource}`,responses:{200:{description:'Current control resource',content:{'application/json':{schema:['state','journal'].includes(resource)?ref('Engine'):{}}}},401:{description:'Missing/invalid token or browser Origin'}}};
  if(resource==='events')path.get.parameters=[{in:'query',name:'after',schema:integer},{in:'query',name:'wait_ms',schema:{...integer,maximum:30000}}];
  if(commands[resource])path.post={operationId:`command_${resource}`,requestBody:{required:true,content:{'application/json':{schema:obj({command_id:{type:'string',minLength:1,maxLength:128},generation:{...integer,minimum:1},payload:commands[resource]},['command_id','payload'])}}},responses:{200:{description:'Applied, or cached response for identical command_id',content:{'application/json':{schema:{}}}},409:{description:'Invalid model command, stale generation, failed assertion or conflicting command_id'},401:{description:'Missing/invalid token or browser Origin'}}};
  paths[`/control/v1/${resource}`]=path;
}
const api={openapi:'3.1.0',info:{title:'PayLink emulator control API',version:'1.0.0',description:'Native test-runner control API. This is not the PayLink wire specification. Browser Origin requests are rejected.'},servers:[{url:'http://127.0.0.1:3001'}],security:[{RunToken:[]}],paths,components:{securitySchemes:{RunToken:{type:'http',scheme:'bearer'}},schemas}};
writeFileSync('docs/control-openapi.json',JSON.stringify(api,null,2)+'\n');
