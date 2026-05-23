/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<object, object, unknown>;
  export default component;
}

declare module '@breezystack/lamejs' {
  export class Mp3Encoder {
    constructor(channels: number, sampleRate: number, bitRate: number);
    encodeBuffer(buffer: Int16Array): Int8Array;
    flush(): Int8Array;
  }
}
