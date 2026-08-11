import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import {
  completeAction,
  createJourney,
  getFeed,
  getJourneys,
  getToday,
  setPostReaction,
} from './src/api/client';
import { CreateJourneyModal } from './src/components/CreateJourneyModal';
import { eventReporter } from './src/analytics/eventReporter';
import { TabBar } from './src/components/TabBar';
import { fallbackFeed, fallbackJourneys, fallbackToday } from './src/data/fallback';
import { DiscoverScreen } from './src/screens/DiscoverScreen';
import { JourneysScreen } from './src/screens/JourneysScreen';
import { ProfileScreen } from './src/screens/ProfileScreen';
import { TodayScreen } from './src/screens/TodayScreen';
import { colors } from './src/theme';
import { CreateJourneyInput, Feed, Journey, TabKey, Today } from './src/types';

export default function App() {
  const [activeTab, setActiveTab] = useState<TabKey>('today');
  const [today, setToday] = useState<Today>(fallbackToday);
  const [journeys, setJourneys] = useState<Journey[]>(fallbackJourneys);
  const [feed, setFeed] = useState<Feed>(fallbackFeed);
  const [likedPostIds, setLikedPostIds] = useState<Set<string>>(
    () => new Set(['post-reading']),
  );
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    eventReporter.start();
    let mounted = true;
    Promise.all([getToday(), getJourneys(), getFeed()])
      .then(([nextToday, nextJourneys, nextFeed]) => {
        if (!mounted) return;
        setToday(nextToday);
        setJourneys(nextJourneys);
        setFeed(nextFeed);
      })
      .catch(() => undefined);
    return () => {
      mounted = false;
      eventReporter.stop();
    };
  }, []);

  const handleComplete = (actionId: string) => {
    setToday((current) => {
      const actions = current.actions.map((action) =>
        action.id === actionId ? { ...action, state: 'completed' as const } : action,
      );
      return {
        ...current,
        actions,
        completed: actions.filter((action) => action.state === 'completed').length,
        focus_minutes: actions
          .filter((action) => action.state === 'completed')
          .reduce((sum, action) => sum + action.estimated_minutes, 0),
      };
    });
    eventReporter.track({ event_type: 'complete', component_id: 'today-action', content_id: actionId });
    completeAction(actionId).catch(() => undefined);
  };

  const handleCreate = (input: CreateJourneyInput) => {
    setCreating(false);
    createJourney(input)
      .then((journey) => setJourneys((current) => [...current, journey]))
      .catch(() => {
        setJourneys((current) => [
          ...current,
          {
            id: `local-${Date.now()}`,
            title: input.title,
            intent: input.intent,
            domain: input.domain,
            status: 'active',
            progress: 0,
            duration_label: input.duration_label,
            next_action: input.first_action_title,
            participant_count: 1,
          },
        ]);
      });
  };

  const handleLike = (postId: string) => {
    const active = !likedPostIds.has(postId);
    setLikedPostIds((current) => {
      const next = new Set(current);
      if (active) next.add(postId);
      else next.delete(postId);
      return next;
    });
    setFeed((current) => ({
      ...current,
      items: current.items.map((item) =>
        item.post.id === postId
          ? { ...item, post: { ...item.post, like_count: Math.max(0, item.post.like_count + (active ? 1 : -1)) } }
          : item,
      ),
    }));
    setPostReaction(postId, active).catch(() => undefined);
    if (active) {
      eventReporter.track({ event_type: 'like', component_id: 'feed-like', content_id: postId });
    }
  };

  const screen = {
    today: <TodayScreen journeys={journeys} onComplete={handleComplete} today={today} />,
    discover: <DiscoverScreen feed={feed} likedPostIds={likedPostIds} onLike={handleLike} />,
    journeys: <JourneysScreen journeys={journeys} onCreate={() => setCreating(true)} />,
    profile: <ProfileScreen />,
  }[activeTab];

  return (
    <SafeAreaProvider>
      <SafeAreaView edges={['top', 'left', 'right']} style={styles.safeArea}>
        <View style={styles.screen}>{screen}</View>
        <TabBar active={activeTab} onChange={setActiveTab} />
      </SafeAreaView>
      <CreateJourneyModal
        onClose={() => setCreating(false)}
        onSubmit={handleCreate}
        visible={creating}
      />
      <StatusBar style="dark" />
    </SafeAreaProvider>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  screen: { flex: 1 },
});
