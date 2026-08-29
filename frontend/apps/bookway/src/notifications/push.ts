import * as Application from 'expo-application';
import Constants from 'expo-constants';
import * as Notifications from 'expo-notifications';
import { Platform } from 'react-native';

import { registerPushDevice } from '../api/client';

Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldPlaySound: false,
    shouldSetBadge: false,
    shouldShowBanner: true,
    shouldShowList: true,
  }),
});

/**
 * Requests native notification permission only after the user enables
 * reminders, then registers the Expo token against the authenticated account.
 * Web, Expo Go without a project ID, and simulators without push capability
 * return false without fabricating a device identity.
 */
export async function registerNativePushDevice(): Promise<boolean> {
  if (Platform.OS !== 'ios' && Platform.OS !== 'android') return false;
  if (Platform.OS === 'android') {
    await Notifications.setNotificationChannelAsync('action-reminders', {
      name: '行动提醒',
      importance: Notifications.AndroidImportance.DEFAULT,
      vibrationPattern: [0, 250, 250, 250],
    });
  }
  const permissions = await Notifications.getPermissionsAsync();
  const finalStatus = permissions.status === 'granted'
    ? permissions.status
    : (await Notifications.requestPermissionsAsync()).status;
  if (finalStatus !== 'granted') return false;
  const projectId = process.env.EXPO_PUBLIC_EAS_PROJECT_ID
    ?? Constants.expoConfig?.extra?.eas?.projectId
    ?? Constants.easConfig?.projectId;
  if (!projectId) return false;
  const token = (await Notifications.getExpoPushTokenAsync({ projectId })).data.trim();
  if (!token) return false;
  const platformId = Platform.OS === 'android'
    ? Application.getAndroidId()
    : await Application.getIosIdForVendorAsync();
  const deviceId = platformId?.trim() || token;
  await registerPushDevice({ device_id: deviceId, provider: 'expo', endpoint: token });
  return true;
}
