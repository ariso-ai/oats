import Pusher from 'pusher-js';
import type { Channel } from 'pusher-js';
import { getDesktopConfig, pusherAuth } from '../tauri';

export interface PusherHandle {
  client: Pusher;
  channel: Channel;
  cleanup: () => void;
}

/**
 * Create an authenticated Pusher client and subscribe to `channelName`.
 * Channel authorization is routed through the Rust api_request proxy
 * (POST /pusher/auth) so the session token is attached server-side.
 */
export async function usePusher(channelName: string): Promise<PusherHandle> {
  const { pusherKey, pusherCluster } = await getDesktopConfig();

  const client = new Pusher(pusherKey, {
    cluster: pusherCluster,
    channelAuthorization: {
      endpoint: '/pusher/auth',
      transport: 'ajax',
      customHandler: async (params, callback) => {
        try {
          const res = await pusherAuth(params.socketId, params.channelName);
          callback(null, res);
        } catch (err) {
          callback(err as Error, null);
        }
      },
    },
  });

  const channel = client.subscribe(channelName);

  function cleanup() {
    channel.unbind_all();
    channel.unsubscribe();
    client.disconnect();
  }

  return { client, channel, cleanup };
}
