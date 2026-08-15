import { EyeOff, X } from 'lucide-react-native';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';

import { colors } from '../theme';
import type { NegativeFeedbackReason } from '../types';

type Props = {
  visible: boolean;
  onClose: () => void;
  onSelect: (reason: NegativeFeedbackReason) => void;
};

const choices: Array<{ reason: NegativeFeedbackReason; title: string; detail: string; color: string; background: string }> = [
  {
    reason: 'not_relevant',
    title: '和我当前要做的事无关',
    detail: '减少同领域和相似路线的推荐',
    color: colors.evergreen,
    background: colors.evergreenSoft,
  },
  {
    reason: 'already_seen',
    title: '已经看过类似内容',
    detail: '降低重复出现，不改变你的兴趣偏好',
    color: colors.blue,
    background: colors.blueSoft,
  },
  {
    reason: 'low_quality',
    title: '内容质量不佳',
    detail: '减少这位创作者的相似输出',
    color: colors.coral,
    background: colors.coralSoft,
  },
];

export function HideFeedbackModal({ visible, onClose, onSelect }: Props) {
  return (
    <Modal animationType="fade" onRequestClose={onClose} transparent visible={visible}>
      <View style={styles.overlay}>
        <Pressable accessibilityLabel="关闭减少推荐原因" onPress={onClose} style={styles.backdrop} />
        <View accessibilityViewIsModal style={styles.sheet}>
          <View style={styles.handle} />
          <View style={styles.heading}>
            <View style={styles.icon}><EyeOff color={colors.evergreen} size={20} /></View>
            <View style={styles.headingCopy}>
              <Text style={styles.title}>帮你调得更准</Text>
              <Text style={styles.subtitle}>选择原因，只影响之后的推荐方式。</Text>
            </View>
            <Pressable accessibilityLabel="关闭减少推荐原因" hitSlop={8} onPress={onClose} style={styles.close}>
              <X color={colors.muted} size={19} />
            </Pressable>
          </View>
          <View style={styles.choices}>
            {choices.map((choice) => (
              <Pressable
                accessibilityLabel={choice.title}
                accessibilityRole="button"
                key={choice.reason}
                onPress={() => onSelect(choice.reason)}
                style={({ pressed }) => [styles.choice, pressed && styles.pressed]}
              >
                <View style={[styles.marker, { backgroundColor: choice.background }]}>
                  <View style={[styles.markerDot, { backgroundColor: choice.color }]} />
                </View>
                <View style={styles.choiceCopy}>
                  <Text style={styles.choiceTitle}>{choice.title}</Text>
                  <Text style={styles.choiceDetail}>{choice.detail}</Text>
                </View>
              </Pressable>
            ))}
          </View>
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end' },
  backdrop: { ...StyleSheet.absoluteFill, backgroundColor: 'rgba(25, 35, 30, 0.38)' },
  sheet: { paddingHorizontal: 20, paddingTop: 10, paddingBottom: 34, borderTopLeftRadius: 20, borderTopRightRadius: 20, backgroundColor: colors.background },
  handle: { alignSelf: 'center', width: 34, height: 4, borderRadius: 2, backgroundColor: colors.line },
  heading: { flexDirection: 'row', alignItems: 'center', gap: 11, marginTop: 18 },
  icon: { width: 42, height: 42, alignItems: 'center', justifyContent: 'center', borderRadius: 12, backgroundColor: colors.evergreenSoft },
  headingCopy: { flex: 1, minWidth: 0 },
  title: { color: colors.ink, fontSize: 17, fontWeight: '700', letterSpacing: 0 },
  subtitle: { color: colors.muted, fontSize: 12, lineHeight: 18, marginTop: 2, letterSpacing: 0 },
  close: { width: 32, height: 32, alignItems: 'center', justifyContent: 'center' },
  choices: { gap: 9, marginTop: 20 },
  choice: { minHeight: 72, paddingHorizontal: 13, flexDirection: 'row', alignItems: 'center', gap: 12, borderRadius: 11, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  pressed: { opacity: 0.64 },
  marker: { width: 30, height: 30, alignItems: 'center', justifyContent: 'center', borderRadius: 15 },
  markerDot: { width: 8, height: 8, borderRadius: 4 },
  choiceCopy: { flex: 1, minWidth: 0 },
  choiceTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  choiceDetail: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 3, letterSpacing: 0 },
});
