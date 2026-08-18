import { CalendarCheck2, Compass, Plus, Route, UserRound, type LucideIcon } from 'lucide-react-native';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';
import { TabKey } from '../types';

const tabs: Array<{ key: TabKey | 'create'; label: string; icon: LucideIcon }> = [
  { key: 'today', label: '今日', icon: CalendarCheck2 },
  { key: 'discover', label: '发现', icon: Compass },
  { key: 'create', label: '创作', icon: Plus },
  { key: 'journeys', label: '路线', icon: Route },
  { key: 'profile', label: '我的', icon: UserRound },
];

type Props = {
  active: TabKey;
  onChange: (tab: TabKey) => void;
  onCreate: () => void;
};

export function TabBar({ active, onChange, onCreate }: Props) {
  return (
    <View style={styles.bar}>
      {tabs.map(({ key, label, icon: Icon }) => {
        const selected = key === 'create' || active === key;
        return (
          <Pressable
            accessibilityRole="tab"
            accessibilityLabel={key === 'create' ? '创作' : label}
            accessibilityState={{ selected: key === 'create' ? false : selected }}
            key={key}
            onPress={() => key === 'create' ? onCreate() : onChange(key)}
            style={({ pressed }) => [styles.item, key === 'create' && styles.createItem, pressed && styles.pressed]}
          >
            {key === 'create' ? <View style={styles.createIcon}><Icon color={colors.surface} size={22} strokeWidth={2.5} /></View> : <Icon color={selected ? colors.evergreen : colors.faint} size={23} strokeWidth={2} />}
            <Text style={[styles.label, selected && key !== 'create' && styles.selected]}>{label}</Text>
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
  createItem: { justifyContent: 'center' },
  createIcon: { width: 36, height: 36, marginTop: -10, marginBottom: -1, alignItems: 'center', justifyContent: 'center', borderRadius: 8, backgroundColor: colors.evergreen },
  pressed: { opacity: 0.55 },
  label: { color: colors.faint, fontSize: 11, fontWeight: '600', letterSpacing: 0 },
  selected: { color: colors.evergreen },
});
