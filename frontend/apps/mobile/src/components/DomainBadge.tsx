import { StyleSheet, Text, View } from 'react-native';

import { domainMeta } from '../theme';
import { GrowthDomain } from '../types';

export function DomainBadge({ domain }: { domain: GrowthDomain }) {
  const meta = domainMeta[domain];
  return (
    <View style={[styles.badge, { backgroundColor: meta.background }]}>
      <Text style={[styles.text, { color: meta.color }]}>{meta.label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  badge: { alignSelf: 'flex-start', paddingHorizontal: 7, paddingVertical: 3, borderRadius: 4 },
  text: { fontSize: 11, lineHeight: 16, fontWeight: '700', letterSpacing: 0 },
});

