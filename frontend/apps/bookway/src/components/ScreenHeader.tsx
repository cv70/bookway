import { Bell, Plus, X } from 'lucide-react-native';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';

type Props = {
  eyebrow?: string;
  title: string;
  action?: 'bell' | 'plus' | 'close';
  badgeCount?: number;
  onAction?: () => void;
};

export function ScreenHeader({ eyebrow, title, action, badgeCount = 0, onAction }: Props) {
  const Icon = action === 'plus' ? Plus : action === 'close' ? X : Bell;
  return (
    <View style={styles.header}>
      <View style={styles.titleGroup}>
        {eyebrow ? <Text style={styles.eyebrow}>{eyebrow}</Text> : null}
        <Text style={styles.title}>{title}</Text>
      </View>
      {action ? (
        <Pressable
          accessibilityLabel={action === 'plus' ? '创建路线' : action === 'close' ? '返回推荐' : badgeCount ? `通知，${badgeCount} 条未读` : '通知'}
          hitSlop={10}
          onPress={onAction}
          style={({ pressed }) => [styles.action, pressed && styles.pressed]}
        >
          <Icon color={colors.ink} size={22} strokeWidth={2} />
          {action === 'bell' && badgeCount ? <View style={styles.badge}><Text style={styles.badgeText}>{badgeCount > 99 ? '99+' : badgeCount}</Text></View> : null}
        </Pressable>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  header: {
    paddingHorizontal: 20,
    paddingTop: 12,
    paddingBottom: 18,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  titleGroup: { flex: 1 },
  eyebrow: {
    color: colors.muted,
    fontSize: 12,
    lineHeight: 18,
    fontWeight: '600',
    letterSpacing: 0,
  },
  title: {
    color: colors.ink,
    fontSize: 27,
    lineHeight: 34,
    fontWeight: '700',
    letterSpacing: 0,
  },
  action: {
    width: 42,
    height: 42,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.line,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  badge: { position: 'absolute', top: -4, right: -4, minWidth: 16, height: 16, paddingHorizontal: 3, borderRadius: 8, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.coral, borderWidth: 1.5, borderColor: colors.background },
  badgeText: { color: colors.surface, fontSize: 8, fontWeight: '800', lineHeight: 11, letterSpacing: 0 },
  pressed: { opacity: 0.55 },
});
