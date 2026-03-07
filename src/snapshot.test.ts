import { describe, it, expect } from 'vitest';
import {
  getEnhancedSnapshot,
  resetRefs,
  parseRef,
  getSnapshotStats,
  type RefMap,
  type EnhancedSnapshot,
} from './snapshot.js';

describe('parseRef', () => {
  it('should parse @e1 format', () => {
    expect(parseRef('@e1')).toBe('e1');
    expect(parseRef('@e42')).toBe('e42');
  });

  it('should parse ref=e1 format', () => {
    expect(parseRef('ref=e1')).toBe('e1');
  });

  it('should parse bare e1 format', () => {
    expect(parseRef('e1')).toBe('e1');
    expect(parseRef('e999')).toBe('e999');
  });

  it('should return null for non-ref strings', () => {
    expect(parseRef('button')).toBeNull();
    expect(parseRef('#id')).toBeNull();
    expect(parseRef('.class')).toBeNull();
  });
});

describe('getSnapshotStats', () => {
  it('should count refs and interactive elements', () => {
    const refs: RefMap = {
      e1: {
        selector: 'getByRole(\'button\', { name: "Submit", exact: true })',
        role: 'button',
        name: 'Submit',
      },
      e2: {
        selector: 'getByRole(\'heading\', { name: "Title", exact: true })',
        role: 'heading',
        name: 'Title',
      },
      e3: {
        selector: 'getByRole(\'link\', { name: "Home", exact: true })',
        role: 'link',
        name: 'Home',
      },
    };
    const tree = '- button "Submit" [ref=e1]\n- heading "Title" [ref=e2]\n- link "Home" [ref=e3]';
    const stats = getSnapshotStats(tree, refs);
    expect(stats.refs).toBe(3);
    expect(stats.interactive).toBe(2); // button + link
    expect(stats.lines).toBe(3);
  });
});

describe('getEnhancedSnapshot - iframe traversal', () => {
  /**
   * Helper to create a mock Frame with ariaSnapshot() support.
   */
  function mockFrame(
    ariaTree: string,
    opts: { name?: string; url?: string; childFrames?: any[] } = {}
  ) {
    return {
      name: () => opts.name ?? '',
      url: () => opts.url ?? 'about:blank',
      childFrames: () => opts.childFrames ?? [],
      locator: (_sel: string) => ({
        ariaSnapshot: async () => ariaTree,
      }),
    };
  }

  /**
   * Helper to create a mock Page wrapping a main frame + child frames.
   */
  function mockPage(mainAriaTree: string, childFrames: ReturnType<typeof mockFrame>[] = []) {
    const mainFrame = mockFrame(mainAriaTree, { childFrames });
    return {
      mainFrame: () => mainFrame,
      locator: (sel: string) => mainFrame.locator(sel),
    };
  }

  it('should produce globally unique refs across main frame and iframes', async () => {
    const mainTree = '- button "Submit"\n- link "Home"';
    const iframeTree = '- button "Pay Now"\n- textbox "Card number"';
    const iframe = mockFrame(iframeTree, {
      name: 'stripe-frame',
      url: 'https://js.stripe.com/v3/checkout',
    });
    const page = mockPage(mainTree, [iframe]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // Collect all refs
    const refIds = Object.keys(snapshot.refs);

    // Should have 4 unique refs (2 main + 2 iframe)
    expect(refIds).toHaveLength(4);
    expect(new Set(refIds).size).toBe(4); // no duplicates

    // Verify no two refs share the same ID
    expect(refIds).toEqual(expect.arrayContaining(['e1', 'e2', 'e3', 'e4']));
  });

  it('should tag iframe refs with frameUrl', async () => {
    const mainTree = '- button "OK"';
    const iframeTree = '- button "Sign In"';
    const iframe = mockFrame(iframeTree, {
      name: 'google-login',
      url: 'https://accounts.google.com/gsi/iframe',
    });
    const page = mockPage(mainTree, [iframe]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // Main frame ref should not have frameUrl
    expect(snapshot.refs['e1'].frameUrl).toBeUndefined();
    expect(snapshot.refs['e1'].name).toBe('OK');

    // Iframe ref should have frameUrl
    expect(snapshot.refs['e2'].frameUrl).toBe('https://accounts.google.com/gsi/iframe');
    expect(snapshot.refs['e2'].name).toBe('Sign In');
  });

  it('should include iframe content indented under a frame label', async () => {
    const mainTree = '- heading "Checkout"';
    const iframeTree = '- textbox "Card number"';
    const iframe = mockFrame(iframeTree, {
      name: 'payment',
      url: 'https://js.stripe.com/v3',
    });
    const page = mockPage(mainTree, [iframe]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // Tree should contain iframe label with name and url
    expect(snapshot.tree).toContain('iframe [name="payment"');
    expect(snapshot.tree).toContain('url="https://js.stripe.com/v3"');
    // iframe content should be indented
    expect(snapshot.tree).toContain('    - textbox "Card number"');
  });

  it('should handle multiple iframes', async () => {
    const mainTree = '- heading "Page"';
    const stripeTree = '- button "Pay"';
    const googleTree = '- button "Sign in with Google"';

    const stripeFrame = mockFrame(stripeTree, {
      url: 'https://js.stripe.com/v3',
    });
    const googleFrame = mockFrame(googleTree, {
      url: 'https://accounts.google.com/gsi/button',
    });
    const page = mockPage(mainTree, [stripeFrame, googleFrame]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // 3 refs total: 1 heading + 1 Pay button + 1 Google button
    expect(Object.keys(snapshot.refs)).toHaveLength(3);

    // Stripe ref
    const stripeRef = Object.values(snapshot.refs).find((r) => r.name === 'Pay');
    expect(stripeRef?.frameUrl).toBe('https://js.stripe.com/v3');

    // Google ref
    const googleRef = Object.values(snapshot.refs).find((r) => r.name === 'Sign in with Google');
    expect(googleRef?.frameUrl).toBe('https://accounts.google.com/gsi/button');
  });

  it('should silently skip iframes that fail ariaSnapshot', async () => {
    const mainTree = '- button "OK"';
    const brokenFrame = {
      name: () => 'broken',
      url: () => 'https://cross-origin.example.com',
      childFrames: () => [],
      locator: () => ({
        ariaSnapshot: async () => {
          throw new Error('cross-origin frame');
        },
      }),
    };
    const page = mockPage(mainTree, [brokenFrame as any]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // Should still have the main frame ref
    expect(Object.keys(snapshot.refs)).toHaveLength(1);
    expect(snapshot.refs['e1'].name).toBe('OK');
  });

  it('should handle empty iframe content gracefully', async () => {
    const mainTree = '- button "Submit"';
    const emptyFrame = mockFrame('', {
      url: 'https://empty.example.com',
    });
    const page = mockPage(mainTree, [emptyFrame]);

    const snapshot = await getEnhancedSnapshot(page as any);

    // Only main frame ref, iframe skipped
    expect(Object.keys(snapshot.refs)).toHaveLength(1);
    expect(snapshot.tree).not.toContain('iframe');
  });

  it('should work with interactive-only mode across iframes', async () => {
    const mainTree = '- heading "Title"\n- button "Submit"\n- paragraph: text';
    const iframeTree = '- heading "Payment"\n- textbox "Card"\n- paragraph: info';
    const iframe = mockFrame(iframeTree, {
      url: 'https://js.stripe.com/v3',
    });
    const page = mockPage(mainTree, [iframe]);

    const snapshot = await getEnhancedSnapshot(page as any, {
      interactive: true,
    });

    // interactive mode: only button + textbox
    const roles = Object.values(snapshot.refs).map((r) => r.role);
    expect(roles).toContain('button');
    expect(roles).toContain('textbox');
    expect(roles).not.toContain('heading');
    expect(roles).not.toContain('paragraph');
  });
});
