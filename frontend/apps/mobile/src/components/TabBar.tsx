import { CalendarCheck2, Compass, Route, UserRound, type LucideIcon } from 'lucide-react-native';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';
import { TabKey } from '../types';

const tabs: Array<{ key: TabKey; label: string; icon: LucideIcon }> = [
  { key: 'today', label: '今日', icon: CalendarCheck2 },
  { key: 'discover', label: '发现', icon: Compass },
  { key: 'journeys', label: '路线', icon: Route },
  { key: 'profile', label: '我的', icon: UserRound },
];

type Props = {
  active: TabKey;
  onChange: (tab: TabKey) => void;
};

export function TabBar({ active, onChange }: Props) {
  return (
    <View style={styles.bar}>
      {tabs.map(({ key, label, icon: Icon }) => {
        const selected = active === key;
        return (
          <Pressable
            accessibilityRole="tab"
            accessibilityState={{ selected }}
            key={key}
            onPress={() => onChange(key)}
            style={({ pressed }) => [styles.item, pressed && styles.pressed]}
          >
            <Icon color={selected ? colors.evergreen : colors.faint} size={23} strokeWidth={2} />
            <Text style={[styles.label, selected && styles.selected]}>{label}</Text>
          </Pressable>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    height: 64,
    flexDirection: 'row',
    alignItems: 'center',
    borderTopColor: colors.line,
    borderTopWidth: StyleSheet.hairlineWidth,
    backgroundColor: colors.surface,
  },
  item: {
    flex: 1,
    height: 64,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 3,
  },
  pressed: { opacity: 0.55 },
  label: { color: colors.faint, fontSize: 11, fontWeight: '600', letterSpacing: 0 },
  selected: { color: colors.evergreen },
});

